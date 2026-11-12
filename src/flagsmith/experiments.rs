//! The running experiments of an environment document, for local evaluation.
//!
//! The document puts an experiment on the feature states it runs on: an enrolled identity
//! matches the experiment's rollout segment, whose override is `in_experiment`, and every other
//! identity gets the environment's feature state, which is not. The engine leaves these out of
//! its results, so they are read from the document and matched to the override it evaluated.

use super::models::ExperimentMetadata;
use flagsmith_flag_engine::engine_eval::context::SegmentContext;
use flagsmith_flag_engine::engine_eval::{
    EngineEvaluationContext, EvaluationResult, SegmentSource,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocumentExperiments {
    /// By feature name: the experiment on the environment's feature state.
    features: HashMap<String, ExperimentMetadata>,
    /// By segment id, then feature name: the experiment on the segment's override.
    segment_overrides: HashMap<i32, HashMap<String, ExperimentMetadata>>,
    /// By identifier: the features of the identity's overrides, which carry no experiment.
    identity_overrides: HashMap<String, HashSet<String>>,
    /// The features with an experiment on any of their feature states: only their flags are
    /// looked up, so evaluation costs nothing more for an environment running none.
    feature_names: HashSet<String>,
}

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    feature_states: Vec<DocumentFeatureState>,
    project: DocumentProject,
    #[serde(default)]
    identity_overrides: Vec<DocumentIdentity>,
}

#[derive(Deserialize)]
struct DocumentProject {
    #[serde(default)]
    segments: Vec<DocumentSegment>,
}

#[derive(Deserialize)]
struct DocumentSegment {
    id: i32,
    #[serde(default)]
    feature_states: Vec<DocumentFeatureState>,
}

#[derive(Deserialize)]
struct DocumentIdentity {
    identifier: String,
    #[serde(default)]
    identity_features: Vec<DocumentFeatureState>,
}

#[derive(Deserialize)]
struct DocumentFeatureState {
    feature: DocumentFeature,
    #[serde(default)]
    metadata: Option<DocumentFeatureStateMetadata>,
}

#[derive(Deserialize)]
struct DocumentFeature {
    name: String,
}

#[derive(Deserialize)]
struct DocumentFeatureStateMetadata {
    #[serde(default)]
    experiment: Option<ExperimentMetadata>,
}

impl DocumentFeatureState {
    fn experiment(self) -> Option<(String, ExperimentMetadata)> {
        let experiment = self.metadata?.experiment?;
        Some((self.feature.name, experiment))
    }
}

impl DocumentExperiments {
    /// Reads the experiments of an environment document. One the SDK cannot read has none.
    pub(crate) fn from_document(document: &serde_json::Value) -> DocumentExperiments {
        let document = match Document::deserialize(document) {
            Ok(document) => document,
            Err(e) => {
                log::warn!("Unable to read the experiments of the environment document: {e}");
                return DocumentExperiments::default();
            }
        };
        let features: HashMap<_, _> = document
            .feature_states
            .into_iter()
            .filter_map(DocumentFeatureState::experiment)
            .collect();
        let segment_overrides: HashMap<i32, HashMap<_, _>> = document
            .project
            .segments
            .into_iter()
            .map(|segment| {
                let experiments: HashMap<_, _> = segment
                    .feature_states
                    .into_iter()
                    .filter_map(DocumentFeatureState::experiment)
                    .collect();
                (segment.id, experiments)
            })
            .filter(|(_, experiments)| !experiments.is_empty())
            .collect();
        let feature_names = features
            .keys()
            .chain(segment_overrides.values().flat_map(HashMap::keys))
            .cloned()
            .collect();
        let identity_overrides = document
            .identity_overrides
            .into_iter()
            .map(|identity| {
                let features = identity
                    .identity_features
                    .into_iter()
                    .map(|feature_state| feature_state.feature.name)
                    .collect();
                (identity.identifier, features)
            })
            .collect();
        DocumentExperiments {
            features,
            segment_overrides,
            identity_overrides,
            feature_names,
        }
    }

    /// The experiments the identity's flags were evaluated under, by feature name, given the
    /// context and result of that evaluation.
    pub(crate) fn for_identity<'a>(
        &'a self,
        context: &EngineEvaluationContext,
        result: &EvaluationResult,
        identifier: &str,
    ) -> Vec<(&'a str, ExperimentMetadata)> {
        if self.feature_names.is_empty() {
            return vec![];
        }
        let identity_overrides = self.identity_overrides.get(identifier);
        // In the order the engine saw them, which breaks ties between overrides.
        let matched_segments: Vec<(i32, &SegmentContext)> = result
            .segments
            .iter()
            .filter(|segment| segment.metadata.source == SegmentSource::Api)
            .filter_map(|segment| {
                let segment_id = segment.metadata.segment_id?;
                Some((segment_id, context.segments.get(&segment_id.to_string())?))
            })
            .collect();
        self.feature_names
            .iter()
            // An identity override takes precedence over every segment override.
            .filter(|feature_name| identity_overrides.is_none_or(|f| !f.contains(*feature_name)))
            .filter_map(|feature_name| {
                let experiment = match winning_segment(&matched_segments, feature_name) {
                    Some(segment_id) => self.segment_overrides.get(&segment_id)?.get(feature_name),
                    None => self.features.get(feature_name),
                }?;
                Some((feature_name.as_str(), experiment.clone()))
            })
            .collect()
    }
}

/// The segment whose override of the feature the engine evaluated: the highest priority one,
/// the first seen winning a tie.
fn winning_segment(matched_segments: &[(i32, &SegmentContext)], feature_name: &str) -> Option<i32> {
    let mut winner: Option<(i32, f64)> = None;
    for (segment_id, segment_context) in matched_segments {
        for feature_override in &segment_context.overrides {
            if feature_override.name != feature_name {
                continue;
            }
            let priority = feature_override.priority.unwrap_or(f64::INFINITY);
            if winner.is_none_or(|(_, incumbent)| priority < incumbent) {
                winner = Some((*segment_id, priority));
            }
        }
    }
    winner.map(|(segment_id, _)| segment_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flagsmith_flag_engine::engine::get_evaluation_result;
    use flagsmith_flag_engine::engine_eval::{add_identity_to_context, environment_to_context};
    use flagsmith_flag_engine::environments::Environment;
    use serde_json::json;

    /// An environment running an experiment on "checkout", whose rollout segment enrolls
    /// identities with trait "enrolled".
    fn document(identity_overrides: serde_json::Value) -> serde_json::Value {
        let feature = json!({"id": 1, "name": "checkout", "type": "MULTIVARIATE"});
        let variants = |percentage: f64| {
            json!([{
                "multivariate_feature_option": {"id": 1, "key": "treatment", "value": "new checkout"},
                "percentage_allocation": percentage,
                "id": 1
            }])
        };
        let experiment = |in_experiment: bool| json!({"experiment": {"id": 7, "name": "checkout experiment", "in_experiment": in_experiment}});
        json!({
            "id": 1,
            "api_key": "environment_key",
            "name": "environment",
            "feature_states": [{
                "feature": feature,
                "enabled": true,
                "django_id": 1,
                "feature_state_value": "control",
                "multivariate_feature_state_values": variants(0.0),
                "metadata": experiment(false)
            }],
            "identity_overrides": identity_overrides,
            "project": {
                "id": 1,
                "name": "project",
                "organisation": {
                    "id": 1, "name": "org", "feature_analytics": false,
                    "stop_serving_flags": false, "persist_trait_data": true
                },
                "hide_disabled_flags": false,
                "segments": [{
                    "id": 3,
                    "name": "experiment-7-rollout",
                    "rules": [{
                        "type": "ALL",
                        "rules": [],
                        "conditions": [{"operator": "IS_SET", "property_": "enrolled", "value": null}]
                    }],
                    "feature_states": [{
                        "feature": feature,
                        "enabled": true,
                        "django_id": 2,
                        "feature_segment": {"priority": 0},
                        "feature_state_value": "control",
                        "multivariate_feature_state_values": variants(100.0),
                        "metadata": experiment(true)
                    }]
                }]
            }
        })
    }

    fn experiment_for(
        document: &serde_json::Value,
        identifier: &str,
        traits: Vec<flagsmith_flag_engine::identities::Trait>,
    ) -> Option<ExperimentMetadata> {
        let environment: Environment = serde_json::from_value(document.clone()).unwrap();
        let context =
            add_identity_to_context(&environment_to_context(environment), identifier, &traits);
        let result = get_evaluation_result(&context);
        DocumentExperiments::from_document(document)
            .for_identity(&context, &result, identifier)
            .into_iter()
            .find(|(feature_name, _)| *feature_name == "checkout")
            .map(|(_, experiment)| experiment)
    }

    fn enrolled() -> Vec<flagsmith_flag_engine::identities::Trait> {
        vec![flagsmith_flag_engine::identities::Trait {
            trait_key: "enrolled".to_string(),
            trait_value: flagsmith_flag_engine::types::FlagsmithValue {
                value: "true".to_string(),
                value_type: flagsmith_flag_engine::types::FlagsmithValueType::Bool,
            },
        }]
    }

    #[test]
    fn an_identity_in_the_rollout_segment_is_in_the_experiment() {
        // When
        let experiment = experiment_for(&document(json!([])), "user_123", enrolled());

        // Then
        assert_eq!(
            experiment,
            Some(ExperimentMetadata {
                id: 7,
                name: "checkout experiment".to_string(),
                in_experiment: true,
            })
        );
    }

    #[test]
    fn an_identity_outside_the_rollout_segment_is_not_in_the_experiment() {
        // When
        let experiment = experiment_for(&document(json!([])), "user_123", vec![]);

        // Then
        assert_eq!(experiment.map(|e| e.in_experiment), Some(false));
    }

    #[test]
    fn an_identity_override_carries_no_experiment() {
        // Given
        let document = document(json!([{
            "identifier": "user_123",
            "identity_uuid": "0f21cde8-63c5-4e50-baca-87897fa6cd01",
            "created_date": "2026-11-12T10:00:00Z",
            "environment_api_key": "environment_key",
            "identity_features": [{
                "feature": {"id": 1, "name": "checkout", "type": "MULTIVARIATE"},
                "enabled": true,
                "featurestate_uuid": "1bddb9a5-7e59-42c6-9be9-625fa369749f",
                "feature_state_value": "overridden"
            }]
        }]));

        // When
        let experiment = experiment_for(&document, "user_123", enrolled());

        // Then
        assert_eq!(experiment, None);
    }

    #[test]
    fn a_document_without_experiments_has_none() {
        // Given
        let document: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/environment.json")).unwrap();

        // When
        let experiments = DocumentExperiments::from_document(&document);

        // Then
        assert!(experiments.feature_names.is_empty());
    }
}
