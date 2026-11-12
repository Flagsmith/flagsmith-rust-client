//! The running experiments of an environment document, for local evaluation.
//!
//! The document puts an experiment on the feature states it runs on: an enrolled identity
//! matches the experiment's rollout segment, whose override is `in_experiment`, and every other
//! identity gets the environment's feature state, which is not. The engine leaves these out of
//! its results, so they are read from the document and matched to the override it evaluated.

use super::models::ExperimentMetadata;
use flagsmith_flag_engine::engine_eval::{
    EngineEvaluationContext, EvaluationResult, SegmentSource,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocumentExperiments {
    /// By feature name: the experiment on the environment's feature state.
    features: HashMap<String, ExperimentMetadata>,
    /// By segment id and feature name: the experiment on the segment's override.
    segment_overrides: HashMap<(i32, String), ExperimentMetadata>,
    /// The identifiers and feature names of identity overrides, which carry no experiment.
    identity_overrides: HashSet<(String, String)>,
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
        DocumentExperiments {
            features: document
                .feature_states
                .into_iter()
                .filter_map(DocumentFeatureState::experiment)
                .collect(),
            segment_overrides: document
                .project
                .segments
                .into_iter()
                .flat_map(|segment| {
                    let id = segment.id;
                    segment
                        .feature_states
                        .into_iter()
                        .filter_map(DocumentFeatureState::experiment)
                        .map(move |(feature_name, experiment)| ((id, feature_name), experiment))
                })
                .collect(),
            identity_overrides: document
                .identity_overrides
                .into_iter()
                .flat_map(|identity| {
                    let identifier = identity.identifier;
                    identity
                        .identity_features
                        .into_iter()
                        .map(move |feature_state| (identifier.clone(), feature_state.feature.name))
                })
                .collect(),
        }
    }

    /// The experiment the identity's flag for the feature was evaluated under, given the
    /// context and result of that evaluation.
    pub(crate) fn for_flag(
        &self,
        context: &EngineEvaluationContext,
        result: &EvaluationResult,
        identifier: &str,
        feature_name: &str,
    ) -> Option<ExperimentMetadata> {
        // An identity override takes precedence over every segment override.
        if self
            .identity_overrides
            .contains(&(identifier.to_string(), feature_name.to_string()))
        {
            return None;
        }
        // The override the engine evaluated: the matched segments' highest priority one,
        // the first seen winning a tie, as matched segments are in the order the engine saw them.
        let mut winner: Option<(i32, f64)> = None;
        for segment in &result.segments {
            if segment.metadata.source != SegmentSource::Api {
                continue;
            }
            let Some(segment_id) = segment.metadata.segment_id else {
                continue;
            };
            let Some(segment_context) = context.segments.get(&segment_id.to_string()) else {
                continue;
            };
            for feature_override in &segment_context.overrides {
                if feature_override.name != feature_name {
                    continue;
                }
                let priority = feature_override.priority.unwrap_or(f64::INFINITY);
                if winner.is_none_or(|(_, incumbent)| priority < incumbent) {
                    winner = Some((segment_id, priority));
                }
            }
        }
        match winner {
            Some((segment_id, _)) => self
                .segment_overrides
                .get(&(segment_id, feature_name.to_string()))
                .cloned(),
            None => self.features.get(feature_name).cloned(),
        }
    }
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
            .for_flag(&context, &result, identifier, "checkout")
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
        assert!(experiments.features.is_empty());
        assert!(experiments.segment_overrides.is_empty());
    }
}
