use flagsmith::flagsmith::events::EventOptions;
use flagsmith::flagsmith::models::SDKTrait;
use flagsmith::flagsmith::offline_handler;
use flagsmith::{Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};

use httpmock::prelude::*;
use rstest::*;
use std::collections::HashMap;

mod fixtures;

use fixtures::default_flag_handler;
use fixtures::environment_json;
use fixtures::environment_json_with_context_value_override;
use fixtures::flags_json;
use fixtures::identities_json;
use fixtures::local_eval_flagsmith;
use fixtures::mock_server;
use fixtures::ENVIRONMENT_KEY;
use fixtures::FEATURE_1_ID;
use fixtures::FEATURE_1_NAME;
use fixtures::SEGMENT_OVERRIDE_VALUE;

#[rstest]
#[should_panic(expected = "default_flag_handler cannot be used with offline_handler")]
fn test_flagsmith_panics_if_both_default_handler_and_offline_hanlder_are_set(
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    let handler =
        offline_handler::LocalFileHandler::new("tests/fixtures/environment.json").unwrap();
    let flagsmith_options = FlagsmithOptions {
        default_flag_handler: Some(default_flag_handler),
        offline_handler: Some(Box::new(handler)),
        ..Default::default()
    };
    Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);
}

#[rstest]
#[should_panic(expected = "offline_handler must be set to use offline_mode")]
fn test_flagsmith_panics_if_offline_mode_is_used_without_offline_hanlder() {
    let flagsmith_options = FlagsmithOptions {
        offline_mode: true,
        ..Default::default()
    };
    Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);
}

#[rstest]
#[should_panic(expected = "offline_handler cannot be used with local evaluation")]
fn test_flagsmith_should_panic_if_local_evaluation_mode_is_used_with_offline_handler() {
    let handler =
        offline_handler::LocalFileHandler::new("tests/fixtures/environment.json").unwrap();
    let flagsmith_options = FlagsmithOptions {
        enable_local_evaluation: true,
        offline_handler: Some(Box::new(handler)),
        ..Default::default()
    };
    Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);
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
fn test_get_environment_flags_ignores_segment_overrides(
    mock_server: MockServer,
    environment_json_with_context_value_override: serde_json::Value,
) {
    // Given: a document with a segment override that would match the environment
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/environment-document/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200)
            .json_body(environment_json_with_context_value_override);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        enable_local_evaluation: true,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith.get_environment_flags().unwrap();
    let flag_value = flags
        .get_feature_value_as_string(fixtures::FEATURE_1_NAME)
        .unwrap();

    // Then: should return environment default value
    assert_eq!(flag_value, fixtures::FEATURE_1_STR_VALUE);
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
fn test_offline_mode() {
    // Given
    let handler =
        offline_handler::LocalFileHandler::new("tests/fixtures/environment.json").unwrap();
    let flagsmith_options = FlagsmithOptions {
        offline_handler: Some(Box::new(handler)),
        ..Default::default()
    };

    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let env_flags = flagsmith.get_environment_flags().unwrap().all_flags();
    let identity_flags = flagsmith
        .get_identity_flags("test_identity", None, None)
        .unwrap()
        .all_flags();

    // Then
    assert_eq!(env_flags.len(), 1);
    assert_eq!(env_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(env_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        env_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );

    // And
    assert_eq!(identity_flags.len(), 1);
    assert_eq!(identity_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(identity_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        identity_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
}

#[rstest]
fn test_offline_handler_is_used_if_request_fails(mock_server: MockServer) {
    let url = mock_server.url("/api/v1/");
    let handler =
        offline_handler::LocalFileHandler::new("tests/fixtures/environment.json").unwrap();
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        offline_handler: Some(Box::new(handler)),
        ..Default::default()
    };

    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let env_flags = flagsmith.get_environment_flags().unwrap().all_flags();
    let identity_flags = flagsmith
        .get_identity_flags("test_identity", None, None)
        .unwrap()
        .all_flags();

    // Then
    assert_eq!(env_flags.len(), 1);
    assert_eq!(env_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(env_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        env_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );

    // And
    assert_eq!(identity_flags.len(), 1);
    assert_eq!(identity_flags[0].feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(identity_flags[0].feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        identity_flags[0].value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
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
        .get_identity_flags("test_identity", None, None)
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
                "traits": [],
                "transient": false,
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
        .get_identity_flags(identifier, None, None)
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
                "traits": [{"trait_key":trait_key, "trait_value": trait_value, "transient": false}],
                "transient": false,
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
    let traits = vec![SDKTrait::new(
        trait_key.to_string(),
        FlagsmithValue {
            value: trait_value.to_string(),
            value_type: FlagsmithValueType::String,
        },
    )];
    let all_flags = flagsmith
        .get_identity_flags(identifier, Some(traits), None)
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
fn test_get_identity_flags_calls_api_when_no_local_environment_with_transient_traits(
    mock_server: MockServer,
    identities_json: serde_json::Value,
) {
    // Given
    let identifier = "test_identity";
    let trait_key = "trait_key1";
    let trait_value = "trait_value1";
    let transient_trait_key = "trait_key2";

    let api_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/identities/")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .header("content-type", "application/json")
            .json_body(serde_json::json!({
                "identifier": identifier,
                "traits": [
                    {"trait_key":trait_key, "trait_value": trait_value, "transient": false},
                    {"trait_key":transient_trait_key, "trait_value": trait_value, "transient": true},
                ],
                "transient": false,
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
    let traits = vec![
        SDKTrait::new(
            trait_key.to_string(),
            FlagsmithValue {
                value: trait_value.to_string(),
                value_type: FlagsmithValueType::String,
            },
        ),
        SDKTrait::new_with_transient(
            transient_trait_key.to_string(),
            FlagsmithValue {
                value: trait_value.to_string(),
                value_type: FlagsmithValueType::String,
            },
            true,
        ),
    ];
    flagsmith
        .get_identity_flags(identifier, Some(traits), None)
        .unwrap()
        .all_flags();

    // Then
    api_mock.assert();
}

#[rstest]
fn test_get_identity_flags_calls_api_when_no_local_environment_with_transient_identity(
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
                "traits": [{"trait_key":trait_key, "trait_value": trait_value, "transient": false}],
                "transient": true,
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
    let traits = vec![SDKTrait::new(
        trait_key.to_string(),
        FlagsmithValue {
            value: trait_value.to_string(),
            value_type: FlagsmithValueType::String,
        },
    )];
    flagsmith
        .get_identity_flags(identifier, Some(traits), Some(true))
        .unwrap()
        .all_flags();

    // Then
    api_mock.assert();
}

#[rstest]
fn test_default_flag_is_not_used_when_environment_flags_returned(
    mock_server: MockServer,
    flags_json: serde_json::Value,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/flags/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body(flags_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith.get_environment_flags().unwrap();
    let flag = flags.get_flag(fixtures::FEATURE_1_NAME).unwrap();
    // Then
    assert_eq!(flag.feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(flag.is_default, false);
    assert_eq!(flag.feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        flag.value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
    assert!(flag.value_as_string().unwrap() != fixtures::DEFAULT_FLAG_HANDLER_FLAG_VALUE);
    api_mock.assert();
}

#[rstest]
fn test_default_flag_is_used_when_no_matching_environment_flag_returned(
    mock_server: MockServer,
    flags_json: serde_json::Value,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/flags/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body(flags_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith.get_environment_flags().unwrap();
    let flag = flags.get_flag("feature_that_does_not_exists").unwrap();
    // Then
    assert_eq!(flag.is_default, true);
    assert!(flag.value_as_string().unwrap() != fixtures::FEATURE_1_STR_VALUE);
    assert_eq!(
        flag.value_as_string().unwrap(),
        fixtures::DEFAULT_FLAG_HANDLER_FLAG_VALUE
    );
    api_mock.assert();
}

#[rstest]
fn test_default_flag_is_not_used_when_identity_flags_returned(
    mock_server: MockServer,
    identities_json: serde_json::Value,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    // Given
    let identifier = "test_identity";
    let api_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/identities/")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .json_body(serde_json::json!({
                "identifier": identifier,
                "traits": [],
                "transient": false,
            }));
        then.status(200).json_body(identities_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith
        .get_identity_flags(identifier, None, None)
        .unwrap();
    let flag = flags.get_flag(fixtures::FEATURE_1_NAME).unwrap();
    // Then
    assert_eq!(flag.feature_name, fixtures::FEATURE_1_NAME);
    assert_eq!(flag.is_default, false);
    assert_eq!(flag.feature_id, fixtures::FEATURE_1_ID);
    assert_eq!(
        flag.value_as_string().unwrap(),
        fixtures::FEATURE_1_STR_VALUE
    );
    assert!(flag.value_as_string().unwrap() != fixtures::DEFAULT_FLAG_HANDLER_FLAG_VALUE);
    api_mock.assert();
}

#[rstest]
fn test_default_flag_is_used_when_no_matching_identity_flags_returned(
    mock_server: MockServer,
    identities_json: serde_json::Value,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    // Given
    let identifier = "test_identity";
    let api_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/identities/")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .json_body(serde_json::json!({
                "identifier": identifier,
                "traits": [],
                "transient": false,
            }));
        then.status(200).json_body(identities_json);
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith
        .get_identity_flags(identifier, None, None)
        .unwrap();
    let flag = flags.get_flag("feature_that_does_not_exists").unwrap();
    // Then
    assert_eq!(flag.is_default, true);
    assert!(flag.value_as_string().unwrap() != fixtures::FEATURE_1_STR_VALUE);
    assert_eq!(
        flag.value_as_string().unwrap(),
        fixtures::DEFAULT_FLAG_HANDLER_FLAG_VALUE
    );
    api_mock.assert();
}

#[rstest]
fn test_default_flags_are_used_if_api_error_and_default_flag_handler_given_for_environment(
    mock_server: MockServer,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    // Give
    let api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/flags/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200).json_body({}); // returning empty body will return api error
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith.get_environment_flags().unwrap();
    let flag = flags.get_flag(fixtures::FEATURE_1_NAME).unwrap();
    // Then
    assert_eq!(flag.is_default, true);
    assert!(flag.value_as_string().unwrap() != fixtures::FEATURE_1_STR_VALUE);
    assert_eq!(
        flag.value_as_string().unwrap(),
        fixtures::DEFAULT_FLAG_HANDLER_FLAG_VALUE
    );
    api_mock.assert();
}

#[rstest]
fn test_default_flags_are_used_if_api_error_and_default_flag_handler_given_for_identity(
    mock_server: MockServer,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    // Given
    let identifier = "test_identity";
    let api_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/identities/")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .json_body(serde_json::json!({
                "identifier": identifier,
                "traits": [],
                "transient": false,
            }));
        then.status(200).json_body({});
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flags = flagsmith
        .get_identity_flags(identifier, None, None)
        .unwrap();
    let flag = flags.get_flag("feature_that_does_not_exists").unwrap();
    // Then
    assert_eq!(flag.is_default, true);
    assert!(flag.value_as_string().unwrap() != fixtures::FEATURE_1_STR_VALUE);
    assert_eq!(
        flag.value_as_string().unwrap(),
        fixtures::DEFAULT_FLAG_HANDLER_FLAG_VALUE
    );
    api_mock.assert();
}

#[rstest]
fn test_flagsmith_api_error_is_returned_if_something_goes_wrong_with_the_request(
    mock_server: MockServer,
) {
    // Give
    let _api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/flags/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(502)
            .json_body(serde_json::json!({"detail": "bad gateway"})); // returning 502
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let err = flagsmith.get_environment_flags().err().unwrap();

    // Then: the error carries the HTTP status and the response body
    assert_eq!(err.kind, flagsmith::error::ErrorKind::FlagsmithAPIError);
    assert!(
        err.msg.contains("502 Bad Gateway"),
        "unexpected msg: {}",
        err.msg
    );
    assert!(
        err.msg.contains("bad gateway"),
        "unexpected msg: {}",
        err.msg
    );
}

#[rstest]
fn test_flagsmith_client_error_is_returned_if_get_flag_is_called_with_a_flag_that_does_not_exists_without_default_handler(
    mock_server: MockServer,
    flags_json: serde_json::Value,
) {
    // Given
    let _api_mock = mock_server.mock(|when, then| {
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
    let err = flagsmith
        .get_environment_flags()
        .unwrap()
        .get_flag("flag_that_does_not_exists")
        .err()
        .unwrap();

    // Then
    assert_eq!(err.kind, flagsmith::error::ErrorKind::FlagsmithAPIError);
}

#[rstest]
fn test_get_identity_segments_no_traits(local_eval_flagsmith: Flagsmith) {
    // Given
    let identifier = "some_identifier";

    // When
    let segments = local_eval_flagsmith
        .get_identity_segments(identifier, None)
        .unwrap();

    //Then
    assert_eq!(segments.len(), 0)
}

#[rstest]
fn test_get_identity_segments_with_valid_trait(local_eval_flagsmith: Flagsmith) {
    // Given
    let identifier = "some_identifier";

    // lifted from fixtures::environment_json
    let trait_key = "foo";
    let trait_value = "bar";

    let traits = vec![Trait {
        trait_key: trait_key.to_string(),
        trait_value: FlagsmithValue {
            value: trait_value.to_string(),
            value_type: FlagsmithValueType::String,
        },
    }];
    // When
    let segments = local_eval_flagsmith
        .get_identity_segments(identifier, Some(traits))
        .unwrap();

    //Then
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].name, "Test Segment");
}

#[rstest]
fn test_get_identity_segments_filters_identity_override_segments(local_eval_flagsmith: Flagsmith) {
    // Given - environment fixture includes identity override for "overridden-id"
    // Use the overridden identity from the fixture
    let identifier = "overridden-id";

    // Test with traits that match the API segment (foo=bar)
    let traits = vec![Trait {
        trait_key: "foo".to_string(),
        trait_value: FlagsmithValue {
            value: "bar".to_string(),
            value_type: FlagsmithValueType::String,
        },
    }];

    // When
    let segments = local_eval_flagsmith
        .get_identity_segments(identifier, Some(traits))
        .unwrap();

    // Then - should only return API segments with source "api",
    assert_eq!(segments.len(), 1, "Should only return API-sourced segments");
    assert_eq!(
        segments[0].name, "Test Segment",
        "Should return the matching API segment"
    );
    assert_eq!(segments[0].id, 1, "Should have correct segment ID");
}

#[rstest]
fn test_reads_do_not_wait_for_a_slow_environment_refresh(
    mock_server: MockServer,
    environment_json: serde_json::Value,
) {
    // Given a document that takes two seconds to arrive and a refresh every 100ms
    let _api_mock = mock_server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/environment-document/")
            .header("X-Environment-Key", ENVIRONMENT_KEY);
        then.status(200)
            .delay(std::time::Duration::from_secs(2))
            .json_body(environment_json);
    });
    let flagsmith_options = FlagsmithOptions {
        api_url: mock_server.url("/api/v1/"),
        enable_local_evaluation: true,
        environment_refresh_interval_mills: 100,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When reads happen while the background refresh is in flight
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(1500) {
        let read = std::time::Instant::now();
        flagsmith.get_environment_flags().unwrap();
        // Then none of them waits for the request: the bound is well under
        // the delay and well over a slow runner's scheduling jitter
        assert!(read.elapsed() < std::time::Duration::from_secs(1));
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[rstest]
fn test_malformed_environment_document_is_an_error_and_keeps_the_last_document(
    mock_server: MockServer,
    environment_json: serde_json::Value,
) {
    // Given a client that has loaded a good document
    let mut good = mock_server.mock(|when, then| {
        when.method(GET).path("/api/v1/environment-document/");
        then.status(200).json_body(environment_json);
    });
    let flagsmith_options = FlagsmithOptions {
        api_url: mock_server.url("/api/v1/"),
        ..Default::default()
    };
    let mut flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);
    flagsmith.update_environment().unwrap();
    let before = flagsmith.get_environment_flags().unwrap().all_flags().len();
    good.delete();

    // When the next document cannot be parsed
    let _bad = mock_server.mock(|when, then| {
        when.method(GET).path("/api/v1/environment-document/");
        then.status(200)
            .json_body(serde_json::json!({"not": "an environment"}));
    });
    let result = flagsmith.update_environment();

    // Then it is an error, and reads still serve the last document
    assert!(result.is_err());
    assert_eq!(
        flagsmith.get_environment_flags().unwrap().all_flags().len(),
        before
    );
}

fn experiment_identities_json(
    enabled: bool,
    variant: Option<&str>,
    in_experiment: bool,
) -> serde_json::Value {
    serde_json::json!({
        "traits": [],
        "flags": [{
            "feature": {"id": FEATURE_1_ID, "name": FEATURE_1_NAME, "type": "MULTIVARIATE"},
            "feature_state_value": "hello treatment",
            "enabled": enabled,
            "variant": variant,
            "metadata": {
                "experiment": {"id": 7, "name": "banner experiment", "in_experiment": in_experiment}
            }
        }]
    })
}

fn events_flagsmith(mock_server: &MockServer) -> Flagsmith {
    let flagsmith_options = FlagsmithOptions {
        api_url: mock_server.url("/api/v1/"),
        enable_events: true,
        events_api_url: mock_server.url("/events/"),
        ..Default::default()
    };
    Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options)
}

#[rstest]
fn test_get_experiment_flag_records_the_exposure_of_an_identity_in_a_running_experiment(
    mock_server: MockServer,
) {
    // Given
    mock_server.mock(|when, then| {
        when.method(POST).path("/api/v1/identities/");
        then.status(200)
            .json_body(experiment_identities_json(true, Some("treatment"), true));
    });
    let events_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/events/v1/events")
            .header("X-Environment-Key", ENVIRONMENT_KEY)
            .json_body_partial(
                serde_json::json!({"events": [{
                    "event": "$flag_exposure",
                    "feature_name": FEATURE_1_NAME,
                    "identifier": "user_123",
                    "value": "treatment",
                    "traits": {"plan": "premium"},
                    "metadata": {"experiment_id": 7},
                }]})
                .to_string(),
            );
        then.status(202);
    });
    let flagsmith = events_flagsmith(&mock_server);
    let traits = vec![SDKTrait::new(
        "plan".to_string(),
        FlagsmithValue {
            value: "premium".to_string(),
            value_type: FlagsmithValueType::String,
        },
    )];

    // When
    let flag = flagsmith
        .get_experiment_flag(FEATURE_1_NAME, "user_123", Some(traits))
        .unwrap();
    flagsmith.flush_events();

    // Then
    assert_eq!(flag.value_as_string().unwrap(), "hello treatment");
    assert_eq!(flag.variant.as_deref(), Some("treatment"));
    events_mock.assert_hits(1);
}

#[rstest]
#[case::identity_outside_the_experiment(true, Some("treatment"), false)]
#[case::disabled_flag(false, Some("treatment"), true)]
#[case::flag_without_a_variant(true, None, true)]
fn test_get_experiment_flag_records_no_exposure_unless_an_enabled_flag_served_an_enrolled_identity_a_variant(
    mock_server: MockServer,
    #[case] enabled: bool,
    #[case] variant: Option<&str>,
    #[case] in_experiment: bool,
) {
    // Given
    mock_server.mock(|when, then| {
        when.method(POST).path("/api/v1/identities/");
        then.status(200)
            .json_body(experiment_identities_json(enabled, variant, in_experiment));
    });
    let events_mock = mock_server.mock(|when, then| {
        when.method(POST).path("/events/v1/events");
        then.status(202);
    });
    let flagsmith = events_flagsmith(&mock_server);

    // When
    let flag = flagsmith
        .get_experiment_flag(FEATURE_1_NAME, "user_123", None)
        .unwrap();
    flagsmith.flush_events();

    // Then
    assert_eq!(flag.enabled, enabled);
    events_mock.assert_hits(0);
}

#[rstest]
fn test_get_experiment_flag_records_no_exposure_for_a_default_flag(
    mock_server: MockServer,
    default_flag_handler: fn(&str) -> flagsmith::Flag,
) {
    // Given
    mock_server.mock(|when, then| {
        when.method(POST).path("/api/v1/identities/");
        then.status(500);
    });
    let events_mock = mock_server.mock(|when, then| {
        when.method(POST).path("/events/v1/events");
        then.status(202);
    });
    let flagsmith_options = FlagsmithOptions {
        api_url: mock_server.url("/api/v1/"),
        enable_events: true,
        events_api_url: mock_server.url("/events/"),
        default_flag_handler: Some(default_flag_handler),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let flag = flagsmith
        .get_experiment_flag(FEATURE_1_NAME, "user_123", None)
        .unwrap();
    flagsmith.flush_events();

    // Then
    assert!(flag.is_default);
    events_mock.assert_hits(0);
}

#[rstest]
fn test_experiment_flags_and_events_require_events_to_be_enabled(mock_server: MockServer) {
    // Given
    let flagsmith_options = FlagsmithOptions {
        api_url: mock_server.url("/api/v1/"),
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let experiment_flag = flagsmith.get_experiment_flag(FEATURE_1_NAME, "user_123", None);
    let event = flagsmith.track_event("checkout_completed", EventOptions::default());
    let exposure = flagsmith.track_exposure_event(
        FEATURE_1_NAME,
        EventOptions {
            identifier: Some("user_123".to_string()),
            ..Default::default()
        },
    );

    // Then
    for error in [
        experiment_flag.unwrap_err(),
        event.unwrap_err(),
        exposure.unwrap_err(),
    ] {
        assert_eq!(
            error.kind,
            flagsmith::error::ErrorKind::FlagsmithClientError
        );
        assert!(error.msg.starts_with("Events must be enabled"));
    }
}

#[rstest]
fn test_track_event_sends_the_event_to_flagsmith(mock_server: MockServer) {
    // Given
    let events_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/events/v1/events")
            .json_body_partial(
                serde_json::json!({"events": [{
                    "event": "checkout_completed",
                    "feature_name": null,
                    "identifier": "user_123",
                    "value": "42",
                    "metadata": {"currency": "GBP"},
                }]})
                .to_string(),
            );
        then.status(202);
    });
    let flagsmith = events_flagsmith(&mock_server);

    // When
    flagsmith
        .track_event(
            "checkout_completed",
            EventOptions {
                identifier: Some("user_123".to_string()),
                value: Some(FlagsmithValue {
                    value: "42".to_string(),
                    value_type: FlagsmithValueType::Integer,
                }),
                metadata: Some(HashMap::from([(
                    "currency".to_string(),
                    serde_json::json!("GBP"),
                )])),
                ..Default::default()
            },
        )
        .unwrap();
    flagsmith.flush_events();

    // Then
    events_mock.assert_hits(1);
}

#[rstest]
fn test_track_event_rejects_reserved_event_names(mock_server: MockServer) {
    // Given
    let flagsmith = events_flagsmith(&mock_server);

    // When
    let result = flagsmith.track_event("$flag_exposure", EventOptions::default());

    // Then
    let error = result.unwrap_err();
    assert_eq!(
        error.kind,
        flagsmith::error::ErrorKind::FlagsmithClientError
    );
    assert!(error.msg.contains("reserved"));
}

#[rstest]
fn test_track_exposure_event_sends_a_flag_exposure_event_to_flagsmith(mock_server: MockServer) {
    // Given
    let events_mock = mock_server.mock(|when, then| {
        when.method(POST)
            .path("/events/v1/events")
            .json_body_partial(
                serde_json::json!({"events": [{
                    "event": "$flag_exposure",
                    "feature_name": "banner",
                    "identifier": "user_123",
                    "value": "control",
                    "metadata": {"experiment_id": 1},
                }]})
                .to_string(),
            );
        then.status(202);
    });
    let flagsmith = events_flagsmith(&mock_server);

    // When
    flagsmith
        .track_exposure_event(
            "banner",
            EventOptions {
                identifier: Some("user_123".to_string()),
                value: Some(FlagsmithValue {
                    value: "control".to_string(),
                    value_type: FlagsmithValueType::String,
                }),
                metadata: Some(HashMap::from([(
                    "experiment_id".to_string(),
                    serde_json::json!(1),
                )])),
                ..Default::default()
            },
        )
        .unwrap();
    flagsmith.flush_events();

    // Then
    events_mock.assert_hits(1);
}

#[rstest]
#[case::no_identifier(None)]
#[case::blank_identifier(Some(" "))]
fn test_track_exposure_event_requires_an_identifier(
    mock_server: MockServer,
    #[case] identifier: Option<&str>,
) {
    // Given
    let flagsmith = events_flagsmith(&mock_server);

    // When
    let result = flagsmith.track_exposure_event(
        "banner",
        EventOptions {
            identifier: identifier.map(str::to_string),
            ..Default::default()
        },
    );

    // Then
    let error = result.unwrap_err();
    assert_eq!(
        error.kind,
        flagsmith::error::ErrorKind::FlagsmithClientError
    );
    assert_eq!(error.msg, "An exposure event requires an identifier.");
}
