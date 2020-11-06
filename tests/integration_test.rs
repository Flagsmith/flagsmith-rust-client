use bullettrain::User;

const API_KEY: &str = "MgfUaRCvvZMznuQyqjnQKt";
const TEST_FEATURE_NAME: &str = "test_feature";
const TEST_FEATURE_VALUE: &str = "sample feature value";
const TEST_USER_FEATURE_VALUE: &str = "user feature value";
const TEST_FLAG_NAME: &str = "test_flag";
const TEST_FLAG_VALUE: bool = true;
const TEST_TRAIT_NAME: &str = "test_trait";
const TEST_TRAIT_VALUE: &str = "sample trait value";
const INVALID_NAME: &str = "invalid_name_for_tests";

fn test_user() -> User {
    User {
        identifier: String::from("test_user"),
    }
}
fn different_user() -> User {
    User {
        identifier: String::from("different_user"),
    }
}

fn get_client() -> bullettrain::Client {
    bullettrain::Client {
        api_key: String::from(API_KEY),
        config: bullettrain::Config {
            base_uri: String::from("https://api.bullet-train.io/api/v1/"),
        },
    }
}

#[test]
fn test_get_features() {
    let features = get_client().get_features().unwrap();
    assert_eq!(features.len(), 4);
    for f in features.iter() {
        assert!(f.feature.name != "");
    }
}

#[test]
fn test_get_user_features() {
    let features = get_client().get_user_features(test_user()).unwrap();
    for f in features.iter() {
        assert!(f.feature.name != "");
    }
}

#[test]
fn test_has_value() {
    let ok = get_client().has_feature(TEST_FEATURE_NAME).unwrap();
    assert!(ok);

    let ok = get_client().has_feature(INVALID_NAME).unwrap();
    assert!(ok == false);
}

#[test]
fn test_feature_enabled() {
    let enabled = get_client().feature_enabled(TEST_FEATURE_NAME).unwrap();
    assert!(!enabled);
    let enabled = get_client().feature_enabled(TEST_FLAG_NAME).unwrap();
    assert!(enabled);
}
