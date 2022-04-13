use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};

use flagsmith::{Flagsmith, FlagsmithOptions};

const ENVIRONMENT_KEY: &str = "ser.test_environment_key";

mod fixtures;
use fixtures::environment_json;
use fixtures::flags_json;
use fixtures::identities_json;
use httpmock::prelude::*;
use rstest::*;

#[fixture]
fn mock_server() -> MockServer {
    MockServer::start()
}

#[rstest]
fn test_get_environment_flags_uses_local_environment_when_available(
    mock_server: MockServer,
    environment_json: serde_json::Value,
) {
    // Given
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/environment-document/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body(environment_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let mut flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When

    flagsmith.update_environment().unwrap();

    // Then
    let all_flags = flagsmith.get_environment_flags().unwrap().all_flags();
    assert_eq!(all_flags.len(), 1);
    assert_eq!(all_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(all_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        all_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
    api_mock.assert();
}

#[rstest]
fn test_get_environment_flags_calls_api_when_no_local_environment(
    mock_server: MockServer,
    flags_json: serde_json::Value,
) {
    // Given
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/flags/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body(flags_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let all_flags = flagsmith.get_environment_flags().unwrap().all_flags();

    // Then
    assert_eq!(all_flags.len(), 1);
    assert_eq!(all_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(all_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        all_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
    api_mock.assert();
}
#[rstest]
fn test_get_identity_flags_uses_local_environment_when_available(
    mock_server: MockServer,
    environment_json: serde_json::Value,
) {
    // Given
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/environment-document/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body(environment_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let mut flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When

    flagsmith.update_environment().unwrap();

    // Then
    let all_flags = flagsmith
        .get_identity_flags("test_identity", None)
        .unwrap()
        .all_flags();
    assert_eq!(all_flags.len(), 1);
    assert_eq!(all_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(all_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        all_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
    api_mock.assert();
}

#[rstest]
fn test_get_identity_flags_calls_api_when_no_local_environment_no_traits(
    mock_server: MockServer,
    identities_json: serde_json::Value,
) {
    // Given
    let identifier = "test_identity";
    let api_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/identities/")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .json_body(serde_json::json!({
                "identifier": identifier,
                "traits": []
            }));
        then.status(200).json_body(identities_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When

    let all_flags = flagsmith
        .get_identity_flags(identifier, None)
        .unwrap()
        .all_flags();

    // Then
    assert_eq!(all_flags.len(), 1);
    assert_eq!(all_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(all_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        all_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );

    api_mock.assert();
}

#[rstest]
fn test_get_identity_flags_calls_api_when_no_local_environment_with_traits(
    mock_server: MockServer,
    identities_json: serde_json::Value,
) {
    // Given
    let identifier = "test_identity";
    let trait_key = "trait_key1";
    let trait_value = "trai_value1";

    let api_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/identities/")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .header("content-type", "application/json")
            .json_body(serde_json::json!({
                "identifier": identifier,
                "traits": [{"trait_key":trait_key, "trait_value": trait_value}]
            }));
        then.status(200).json_body(identities_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let traits = vec![Trait {
        trait_key: trait_key.to_string(),
        trait_value: FlagsmithValue {
            value: trait_value.to_string(),
            value_type: FlagsmithValueType::String,
        },
    }];
    let all_flags = flagsmith
        .get_identity_flags(identifier, Some(traits))
        .unwrap()
        .all_flags();

    // Then
    assert_eq!(all_flags.len(), 1);
    assert_eq!(all_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(all_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        all_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );

    api_mock.assert();
}
