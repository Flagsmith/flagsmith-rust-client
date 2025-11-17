use crate::flagsmith::analytics::AnalyticsProcessor;
use core::f64;
use flagsmith_flag_engine::engine_eval::EvaluationResult;
use flagsmith_flag_engine::features::FeatureState;
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error;

#[derive(Clone, Debug, Default)]
pub struct Flag {
    pub enabled: bool,
    pub value: FlagsmithValue,
    pub is_default: bool,
    pub feature_id: u32,
    pub feature_name: String,
}

impl Flag {
    pub fn from_feature_state(feature_state: FeatureState, identity_id: Option<&str>) -> Flag {
        return Flag {
            enabled: feature_state.enabled,
            value: feature_state.get_value(identity_id),
            is_default: false,
            feature_name: feature_state.feature.name,
            feature_id: feature_state.feature.id,
        };
    }

    pub fn from_api_flag(flag_json: &serde_json::Value) -> Option<Flag> {
        let value: FlagsmithValue =
            serde_json::from_value(flag_json["feature_state_value"].clone()).ok()?;

        let flag = Flag {
            enabled: flag_json["enabled"].as_bool()?,
            is_default: false,
            feature_name: flag_json["feature"]["name"].as_str()?.to_string(),
            feature_id: flag_json["feature"]["id"].as_u64()?.try_into().ok()?,
            value,
        };
        Some(flag)
    }
    pub fn value_as_string(&self) -> Option<String> {
        match self.value.value_type {
            FlagsmithValueType::String => Some(self.value.value.clone()),
            _ => None,
        }
    }
    pub fn value_as_bool(&self) -> Option<bool> {
        match self.value.value_type {
            FlagsmithValueType::Bool => match self.value.value.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn value_as_f64(&self) -> Option<f64> {
        match self.value.value_type {
            FlagsmithValueType::Float => Some(self.value.value.parse::<f64>().ok()?),
            _ => None,
        }
    }
    pub fn value_as_i64(&self) -> Option<i64> {
        match self.value.value_type {
            FlagsmithValueType::Integer => Some(self.value.value.parse::<i64>().ok()?),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Flags {
    flags: HashMap<String, Flag>,
    analytics_processor: Option<AnalyticsProcessor>,
    default_flag_handler: Option<fn(&str) -> Flag>,
}

impl Flags {
    pub fn from_feature_states(
        feature_states: &Vec<FeatureState>,
        analytics_processor: Option<AnalyticsProcessor>,
        default_flag_handler: Option<fn(&str) -> Flag>,
        identity_id: Option<&str>,
    ) -> Flags {
        let mut flags: HashMap<String, Flag> = HashMap::new();
        for feature_state in feature_states {
            flags.insert(
                feature_state.feature.name.clone(),
                Flag::from_feature_state(feature_state.to_owned(), identity_id),
            );
        }
        return Flags {
            flags,
            analytics_processor,
            default_flag_handler,
        };
    }
    pub fn from_api_flags(
        api_flags: &Vec<serde_json::Value>,
        analytics_processor: Option<AnalyticsProcessor>,
        default_flag_handler: Option<fn(&str) -> Flag>,
    ) -> Option<Flags> {
        let mut flags: HashMap<String, Flag> = HashMap::new();
        for flag_json in api_flags {
            let flag = Flag::from_api_flag(flag_json)?;
            flags.insert(flag.feature_name.clone(), flag);
        }
        return Some(Flags {
            flags,
            analytics_processor,
            default_flag_handler,
        });
    }

    pub fn from_evaluation_result(
        result: &EvaluationResult,
        analytics_processor: Option<AnalyticsProcessor>,
        default_flag_handler: Option<fn(&str) -> Flag>,
    ) -> Flags {
        let mut flags: HashMap<String, Flag> = HashMap::new();
        for (feature_name, flag_result) in &result.flags {
            let flag = Flag {
                feature_name: flag_result.name.clone(),
                is_default: false,
                enabled: flag_result.enabled,
                value: flag_result.value.clone(),
                feature_id: flag_result.metadata.feature_id,
            };
            flags.insert(feature_name.clone(), flag);
        }
        return Flags {
            flags,
            analytics_processor,
            default_flag_handler,
        };
    }

    // Returns a vector of all `Flag` structs
    pub fn all_flags(&self) -> Vec<Flag> {
        return self.flags.clone().into_values().collect();
    }

    // Check whether a given feature is enabled.
    // Returns error:Error if the feature is not found
    pub fn is_feature_enabled(&self, feature_name: &str) -> Result<bool, error::Error> {
        Ok(self.get_flag(feature_name)?.enabled)
    }

    // Returns the string value of a given feature
    // Or error if the feature is not found
    pub fn get_feature_value_as_string(&self, feature_name: &str) -> Result<String, error::Error> {
        let flag = self.get_flag(feature_name)?;
        return Ok(flag.value.value);
    }

    // Returns a specific `Flag` given the feature name
    pub fn get_flag(&self, feature_name: &str) -> Result<Flag, error::Error> {
        match self.flags.get(&feature_name.to_string()) {
            Some(flag) => {
                if self.analytics_processor.is_some() && !flag.is_default {
                    let _ = self
                        .analytics_processor
                        .as_ref()
                        .unwrap()
                        .tx
                        .send(flag.feature_name.clone());
                };
                return Ok(flag.clone());
            }
            None => match self.default_flag_handler {
                Some(handler) => Ok(handler(feature_name)),
                None => Err(error::Error::new(
                    error::ErrorKind::FlagsmithAPIError,
                    "API returned invalid response".to_string(),
                )),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SDKTrait {
    pub trait_key: String,
    pub trait_value: FlagsmithValue,
    #[serde(default)]
    pub transient: bool,
}

impl SDKTrait {
    pub fn new(trait_key: String, trait_value: FlagsmithValue) -> SDKTrait {
        return SDKTrait {
            trait_key: trait_key,
            trait_value: trait_value,
            transient: Default::default(),
        };
    }
    pub fn new_with_transient(
        trait_key: String,
        trait_value: FlagsmithValue,
        transient: bool,
    ) -> Self {
        return SDKTrait {
            trait_key: trait_key,
            trait_value: trait_value,
            transient: transient,
        };
    }
}

impl From<SDKTrait> for Trait {
    fn from(t: SDKTrait) -> Self {
        Self {
            trait_key: t.trait_key,
            trait_value: t.trait_value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static FEATURE_STATE_JSON_STRING: &str = r#"{
            "multivariate_feature_state_values": [
        {
            "id": 3404,
            "multivariate_feature_option": {
              "value": "baz"
            },
            "percentage_allocation": 30
          }
            ],
            "feature_state_value": 1,
            "django_id": 1,
            "feature": {
                "name": "feature1",
                "type": null,
                "id": 1
            },
            "segment_id": null,
            "enabled": false
        }"#;

    #[test]
    fn can_create_flag_from_feature_state() {
        // Given
        let feature_state: FeatureState =
            serde_json::from_str(FEATURE_STATE_JSON_STRING).unwrap();
        // When
        let flag = Flag::from_feature_state(feature_state.clone(), None);
        // Then
        assert_eq!(flag.feature_name, feature_state.feature.name);
        assert_eq!(flag.is_default, false);
        assert_eq!(flag.enabled, feature_state.enabled);
        assert_eq!(flag.value, feature_state.get_value(None));
        assert_eq!(flag.feature_id, feature_state.feature.id);
    }

    #[test]
    fn can_create_flag_from_from_api_flag() {
        // Give
        let feature_state_json: serde_json::Value =
            serde_json::from_str(FEATURE_STATE_JSON_STRING).unwrap();
        let expected_value: FlagsmithValue =
            serde_json::from_value(feature_state_json["feature_state_value"].clone()).unwrap();

        // When
        let flag = Flag::from_api_flag(&feature_state_json).unwrap();

        // Then
        assert_eq!(
            flag.feature_name,
            feature_state_json["feature"]["name"].as_str().unwrap()
        );
        assert_eq!(
            flag.feature_id,
            feature_state_json["feature"]["id"].as_u64().unwrap() as u32
        );
        assert_eq!(flag.is_default, false);
        assert_eq!(
            flag.enabled,
            feature_state_json["enabled"].as_bool().unwrap()
        );
        assert_eq!(flag.value, expected_value);
    }

    #[test]
    fn value_as_string() {
        // Give
        let feature_state_json = serde_json::json!({
            "multivariate_feature_state_values": [],
            "feature_state_value": "test_value",
            "django_id": 1,
            "feature": {
                "name": "feature1",
                "type": null,
                "id": 1
            },
            "segment_id": null,
            "enabled": false
        });

        // When
        let flag = Flag::from_api_flag(&feature_state_json).unwrap();

        // Then
        assert_eq!(flag.value_as_string().unwrap(), "test_value");
    }

    #[test]
    fn value_as_bool() {
        // Give
        let feature_state_json = serde_json::json!({
            "multivariate_feature_state_values": [],
            "feature_state_value": true,
            "django_id": 1,
            "feature": {
                "name": "feature1",
                "type": null,
                "id": 1
            },
            "segment_id": null,
            "enabled": false
        });

        // When
        let flag = Flag::from_api_flag(&feature_state_json).unwrap();

        // Then
        assert_eq!(flag.value_as_bool().unwrap(), true);
    }

    #[test]
    fn value_as_i64() {
        // Give
        let feature_state_json = serde_json::json!({
            "multivariate_feature_state_values": [],
            "feature_state_value": 10,
            "django_id": 1,
            "feature": {
                "name": "feature1",
                "type": null,
                "id": 1
            },
            "segment_id": null,
            "enabled": false
        });

        // When
        let flag = Flag::from_api_flag(&feature_state_json).unwrap();

        // Then
        assert_eq!(flag.value_as_i64().unwrap(), 10);
    }

    #[test]
    fn value_as_f64() {
        // Give
        let feature_state_json = serde_json::json!({
            "multivariate_feature_state_values": [],
            "feature_state_value": 10.1,
            "django_id": 1,
            "feature": {
                "name": "feature1",
                "type": null,
                "id": 1
            },
            "segment_id": null,
            "enabled": false
        });

        // When
        let flag = Flag::from_api_flag(&feature_state_json).unwrap();

        // Then
        assert_eq!(flag.value_as_f64().unwrap(), 10.1);
    }
    #[test]
    fn value_as_type_returns_none_if_value_is_of_a_different_type() {
        // Give
        let feature_state_json = serde_json::json!({
            "multivariate_feature_state_values": [],
            "feature_state_value": 10.1,
            "django_id": 1,
            "feature": {
                "name": "feature1",
                "type": null,
                "id": 1
            },
            "segment_id": null,
            "enabled": false
        });

        // When
        let flag = Flag::from_api_flag(&feature_state_json).unwrap();

        // Then
        assert_eq!(flag.value_as_i64().is_none(), true);
    }
}
