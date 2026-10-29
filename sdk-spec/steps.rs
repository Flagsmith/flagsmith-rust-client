//! Steps for the Flagsmith SDK behavioural spec: https://github.com/Flagsmith/sdk-spec
//!
//! Each function implements the Gherkin step in the comment above it.

use std::collections::HashMap;

use flagsmith::error::Error;
use flagsmith::flagsmith::events::EventOptions;
use flagsmith::flagsmith::models::{Flag, Flags, SDKTrait};
use flagsmith::{Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::types::FlagsmithValue;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct SpecFlag {
    feature: String,
    enabled: bool,
    value: FlagsmithValue,
}

fn as_spec_flag(flag: Flag) -> SpecFlag {
    SpecFlag {
        feature: flag.feature_name,
        enabled: flag.enabled,
        value: flag.value,
    }
}

fn as_spec_flags(flags: Flags) -> Vec<SpecFlag> {
    flags.all_flags().into_iter().map(as_spec_flag).collect()
}

fn as_sdk_traits(traits: HashMap<String, FlagsmithValue>) -> Vec<SDKTrait> {
    traits
        .into_iter()
        .map(|(key, value)| SDKTrait::new(key, value))
        .collect()
}

// Create the SDK client
pub fn create_client(environment_key: String, api_url: String) -> Flagsmith {
    let options = FlagsmithOptions {
        api_url,
        ..Default::default()
    };
    Flagsmith::new(environment_key, options)
}

// Create the SDK client with events enabled
pub fn create_client_with_events(
    environment_key: String,
    api_url: String,
    events_api_url: String,
) -> Flagsmith {
    let options = FlagsmithOptions {
        api_url,
        enable_events: true,
        events_api_url,
        ..Default::default()
    };
    Flagsmith::new(environment_key, options)
}

// When the application gets the environment flags
pub fn get_environment_flags(flagsmith: &Flagsmith) -> Result<Vec<SpecFlag>, Error> {
    flagsmith.get_environment_flags().map(as_spec_flags)
}

// When the application gets the flags for identity "{identifier}" with traits:
pub fn get_identity_flags(
    flagsmith: &Flagsmith,
    identifier: String,
    traits: HashMap<String, FlagsmithValue>,
) -> Result<Vec<SpecFlag>, Error> {
    flagsmith
        .get_identity_flags(&identifier, Some(as_sdk_traits(traits)), None)
        .map(as_spec_flags)
}

// When the application gets the experiment flag "{feature}" for identity "{identifier}" with traits:
pub fn get_experiment_flag(
    flagsmith: &Flagsmith,
    feature: String,
    identifier: String,
    traits: HashMap<String, FlagsmithValue>,
) -> Result<Vec<SpecFlag>, Error> {
    flagsmith
        .get_experiment_flag(&feature, &identifier, Some(as_sdk_traits(traits)))
        .map(|flag| vec![as_spec_flag(flag)])
}

// When the application tracks event "{event}" for identity "{identifier}" with value {value} and metadata:
pub fn track_event(
    flagsmith: &Flagsmith,
    event: String,
    identifier: String,
    value: FlagsmithValue,
    metadata: HashMap<String, Value>,
) -> Result<(), Error> {
    let options = EventOptions {
        identifier: Some(identifier),
        value: Some(value),
        metadata: Some(metadata),
        ..Default::default()
    };
    flagsmith.track_event(&event, options)
}

// When the application tracks an exposure of feature "{feature}" for identity "{identifier}" with value {value} and metadata:
pub fn track_exposure_event(
    flagsmith: &Flagsmith,
    feature: String,
    identifier: String,
    value: FlagsmithValue,
    metadata: HashMap<String, Value>,
) -> Result<(), Error> {
    let options = EventOptions {
        identifier: Some(identifier),
        value: Some(value),
        metadata: Some(metadata),
        ..Default::default()
    };
    flagsmith.track_exposure_event(&feature, options)
}
