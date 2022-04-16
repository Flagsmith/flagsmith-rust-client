use rstest::*;
use serde_json;
pub static FEATURE_1_NAME: &str = "feature_1";
pub static FEATURE_1_ID: u32 = 1;
pub static FEATURE_1_STR_VALUE: &str = "some_value";

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
                "segments": []
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
