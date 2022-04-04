use std::{collections::HashMap};
use flagsmith_flag_engine::features::FeatureState;

pub struct DefaultFlag {
    enabled: bool,
    value: String,
    is_default: bool,
}

#[derive(Clone, Debug)]
pub struct Flag {
    enabled: bool,
    value: String,
    is_default: bool,
    feature_id: u32,
    feature_name: String,
}

impl Flag {
    pub fn from_feature_state(feature_state: FeatureState, identity_id: Option<&str>) -> Flag {
        return Flag {
            enabled: feature_state.enabled,
            value: feature_state.get_value(identity_id).value,
            is_default: false,
            feature_name: feature_state.feature.name,
            feature_id: feature_state.feature.id,
        };
    }

    // pub fn from_api_flags() -> Flag {}

}

#[derive(Clone, Debug)]
pub struct Flags {
    flags: HashMap<String, Flag>,
    //TODO: Add default_flag_handler
    // TODO: Add _analytics_processor
}
impl Flags {
    //TODO Add analytics and default flag
    pub fn from_feature_states(feature_states: &Vec<FeatureState>, identity_id: Option<&str>) -> Flags {
        let mut flags: HashMap<String, Flag> = HashMap::new();
        for feature_state in feature_states{
            flags.insert(feature_state.feature.name.clone(), Flag::from_feature_state(feature_state.to_owned(), identity_id));
        }
        return Flags{flags}
    }
    // pub fn from_api_flags() -> Flags {}
    // pub fn all_flags(&self) ->Vec<Flag>{}
    // pub fn is_feature_enabled(&self, feature_name: &str)-> bool{}
    // pub fn get_feature_value(&self, feature_name: &str) -> String {}
    // pub fn get_flag(&self, feature_name: &str) -> Flag {}

}
