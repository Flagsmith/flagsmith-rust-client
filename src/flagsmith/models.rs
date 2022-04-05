use crate::flagsmith::analytics::AnalyticsProcessor;
use flagsmith_flag_engine::features::FeatureState;
use flagsmith_flag_engine::types::FlagsmithValue;
use std::collections::HashMap;

use crate::error;

pub struct DefaultFlag {
    enabled: bool,
    value: String,
    is_default: bool,
}

#[derive(Clone, Debug)]
pub struct Flag {
    enabled: bool,
    value: FlagsmithValue,
    is_default: bool,
    feature_id: u32,
    feature_name: String,
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
            serde_json::from_value(flag_json["feature_state_value"]).ok()?;
        let flag = Flag {
            enabled: flag_json["enabled"].as_bool()?,
            is_default: false,
            feature_name: flag_json["feature"]["name"].as_str()?.to_string(),
            feature_id: flag_json["feature"]["id"].as_u64()?.try_into().ok()?,
            value,
        };
        Some(flag)
    }
}

#[derive(Clone, Debug)]
pub struct Flags {
    flags: HashMap<String, Flag>,
    analytics_processor: Option<AnalyticsProcessor>,
    default_flag_handler: Option<fn(String) -> Flag>,
}
impl Flags {
    pub fn from_feature_states(
        feature_states: &Vec<FeatureState>,
        analytics_processor: Option<AnalyticsProcessor>,
        default_flag_handler: Option<fn(String) -> Flag>,
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
        default_flag_handler: Option<fn(String) -> Flag>,
    ) -> Option<Flags> {
        let mut flags: HashMap<String, Flag> = HashMap::new();
        for flag_json in api_flags {
            let flag = Flag::from_api_flag(flag_json)?;
            flags.insert(flag.feature_name, flag);
        }
        return Some(Flags {
            flags,
            analytics_processor,
            default_flag_handler,
        });
    }

    // Returns a vector of all flags values
    pub fn all_flags(&self) -> Vec<Flag> {
        return self.flags.into_values().collect();
    }
    pub fn is_feature_enabled(&self, feature_name: String) -> Result<bool, error::Error> {
        Ok(self.get_flag(feature_name)?.enabled)
    }
    pub fn get_flag(&self, feature_name: String) -> Result<Flag, error::Error> {
        match self.flags.get(&feature_name) {
            Some(flag) => Ok(flag.clone()),
            None => {
                match self.default_flag_handler {
                    Some(handler) => Ok(handler(feature_name)),
                    None => Err(error::Error::new(
                        error::ErrorKind::FlagsmithAPIError,
                        "API returned invalid response".to_string(),
                    )),
                }

                // if self.default_flag_handler.is_some(){
                //     (Ok(self.default_flag_handler))(feature_name)
                // }
                //Err(error::Error::new(error::ErrorKind::FlagsmithAPIError,"API returned invalid response".to_string()))
            }
        }
        // return flag
    }
    // pub fn from_api_flags() -> Flags {}
    // pub fn all_flags(&self) ->Vec<Flag>{}
    // pub fn is_feature_enabled(&self, feature_name: &str)-> bool{}
    // pub fn get_feature_value(&self, feature_name: &str) -> String {}
    // pub fn get_flag(&self, feature_name: &str) -> Flag {}
}
