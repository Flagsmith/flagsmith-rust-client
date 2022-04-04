use flagsmith_flag_engine::features::FeatureState;

pub struct DefaultFlag {
    enabled: bool,
    value: String,
    is_default: bool,
}

pub struct Flag {
    enabled: bool,
    value: String,
    is_default: bool,
    feature_id: u32,
    feature_name: String,
}

impl Flag {
    pub fn from_feature_state_model(feature_state: FeatureState, identity_id: Option<str>) -> Flag {
        return Flag {
            enabled: feature_state.enabled,
            value: feature_state.get_value(identity_id),
            is_default: false,
            feature_name: feature_state.feature.name,
            feature_id: feature_state.feature.id,
        };
    }

    pub fn from_api_flags() -> Flag {}
}

pub struct Flags {
    flags: Vec<Flag>,
    //TODO: Add default_flag_handler
    // TODO: Add _analytics_processor
}
impl Flags {
    pub fn from_feature_states() -> Flags {}
    pub fn from_api_flags() -> Flags {}
    pub fn all_flags(&self) ->Vec<Flag>{}
    pub fn is_feature_enabled(&self, feature_name:str)-> bool{}
    pub fn get_feature_value(&self, feature_name:str) -> String {}
    pub fn get_flag(&self, feature_name:str) -> Flag {}
}
