use httpmock::prelude::*;
use rstest::*;
use serde_json;

use flagsmith::{Flagsmith, FlagsmithOptions};
pub static FEATURE_1_NAME: &str = "feature_1";
pub static FEATURE_1_ID: u32 = 1;
pub static FEATURE_1_STR_VALUE: &str = "some_value";
pub static DEFAULT_FLAG_HANDLER_FLAG_VALUE: &str = "default_flag_handler_flag_value";

pub const ENVIRONMENT_KEY: &str = "ser.test_environment_key";

#[fixture]
pub fn environment_json() -> serde_json::Value {
    serde_json::json!({
            "api_key": "B62qaMZNwfiqT76p38ggrQ",
            "project": {
                "name": "Test project",
                "organisation": {
                    "feature_analytics": false,
                    "name": "Test Org",
                    "id": 1,
                    "persist_trait_data": true,
                    "stop_serving_flags": false
                },
                "id": 1,
                "hide_disabled_flags": false,
                "segments": [
                    {
                        "id": 1,
                        "name": "Test Segment",
                        "feature_states":[],
                        "rules": [
                            {
                                "type": "ALL",
                                "conditions": [],
                                "rules": [
                                    {
                                        "type": "ALL",
                                        "rules": [],
                                        "conditions": [
                                            {
                                                "operator": "EQUAL",
                                                "property_": "foo",
                                                "value": "bar"
                                            }
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            },
            "segment_overrides": [],
            "id": 1,
            "feature_states": [
                {
                    "multivariate_feature_state_values": [],
                    "feature_state_value": FEATURE_1_STR_VALUE,
                    "id": 1,
                    "featurestate_uuid": "40eb539d-3713-4720-bbd4-829dbef10d51",
                    "feature": {
                        "name": FEATURE_1_NAME,
                        "type": "STANDARD",
                        "id": FEATURE_1_ID
                    },
                    "segment_id": null,
                    "enabled": true
                }
            ]
    })
}

#[fixture]
pub fn flags_json() -> serde_json::Value {
    serde_json::json!(
            [
                {
                    "id": 1,
                    "feature": {
                        "id": FEATURE_1_ID,
                        "name": FEATURE_1_NAME,
                        "created_date": "2019-08-27T14:53:45.698555Z",
                        "initial_value": null,
                        "description": null,
                        "default_enabled": false,
                        "type": "STANDARD",
                        "project": 1
                    },
                    "feature_state_value": FEATURE_1_STR_VALUE,
                    "enabled": true,
                    "environment": 1,
                    "identity": null,
                    "feature_segment": null
                }
    ]
        )
}

#[fixture]
pub fn identities_json() -> serde_json::Value {
    serde_json::json!(
            {
                "traits": [
                    {
                        "id": 1,
                        "trait_key": "some_trait",
                        "trait_value": "some_value"
                    }
                ],
                "flags": [
                    {
                        "id": 1,
                        "feature": {
                            "id": FEATURE_1_ID,
                            "name": FEATURE_1_NAME,
                            "created_date": "2019-08-27T14:53:45.698555Z",
                            "initial_value": null,
                            "description": null,
                            "default_enabled": false,
                            "type": "STANDARD",
                            "project": 1
                        },
                        "feature_state_value": FEATURE_1_STR_VALUE,
                        "enabled": true,
                        "environment": 1,
                        "identity": null,
                        "feature_segment": null
                    }
                ]
    }
        )
}

#[fixture]
pub fn default_flag_handler() -> fn(&str) -> flagsmith::Flag {
    fn handler(_feature_name: &str) -> flagsmith::Flag {
        let mut default_flag = flagsmith::Flag::default();
        default_flag.enabled = true;
        default_flag.is_default = true;
        default_flag.value.value_type = flagsmith_flag_engine::types::FlagsmithValueType::String;
        default_flag.value.value = DEFAULT_FLAG_HANDLER_FLAG_VALUE.to_string();
        return default_flag;
    }
    return handler;
}

#[fixture]
pub fn mock_server() -> MockServer {
    MockServer::start()
}

#[fixture]
pub fn local_eval_flagsmith(
    environment_json: serde_json::Value,
    mock_server: MockServer,
) -> Flagsmith {
    // Given
    let _api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/environment-document/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body(environment_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        enable_local_evaluation: true,
        ..Default::default()
    };
    let mut flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    flagsmith.update_environment().unwrap();
    return flagsmith;
}
