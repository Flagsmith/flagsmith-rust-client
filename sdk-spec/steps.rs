//! Steps for the Flagsmith SDK behavioural spec: https://github.com/Flagsmith/sdk-spec
//!
//! Each function implements the Gherkin step in the comment above it.

use std::collections::HashMap;

use flagsmith::error::Error;
use flagsmith::flagsmith::models::{Flags, SDKTrait};
use flagsmith::{Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::types::FlagsmithValue;
use serde::Serialize;

#[derive(Serialize)]
pub struct SpecFlag {
    feature: String,
    enabled: bool,
    value: FlagsmithValue,
}

fn as_spec_flags(flags: Flags) -> Vec<SpecFlag> {
    flags
        .all_flags()
        .into_iter()
        .map(|flag| SpecFlag {
            feature: flag.feature_name,
            enabled: flag.enabled,
            value: flag.value,
        })
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
    let traits = traits
        .into_iter()
        .map(|(key, value)| SDKTrait::new(key, value))
        .collect();
    flagsmith
        .get_identity_flags(&identifier, Some(traits), None)
        .map(as_spec_flags)
}
