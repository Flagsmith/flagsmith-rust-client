use flagsmith::{Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};

use httpmock::prelude::*;
use rstest::*;

mod fixtures;

use fixtures::default_flag_handler;
use fixtures::environment_json;
use fixtures::flags_json;
use fixtures::identities_json;
use fixtures::local_eval_flagsmith;
use fixtures::mock_server;
use fixtures::ENVIRONMENT_KEY;

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
                "traits": []
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
    let flags = flagsmith.get_identity_flags(identifier, None).unwrap();
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
                "traits": []
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
    let flags = flagsmith.get_identity_flags(identifier, None).unwrap();
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
                "traits": []
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
    let flags = flagsmith.get_identity_flags(identifier, None).unwrap();
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
        then.status(502).json_body({}); // returning 502
    });
    let url = mock_server.url("/api/v1/");
    let flagsmith_options = FlagsmithOptions {
        api_url: url,
        ..Default::default()
    };
    let flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    // When
    let err = flagsmith.get_environment_flags().err().unwrap();
    assert_eq!(err.kind, flagsmith::error::ErrorKind::FlagsmithAPIError);
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
