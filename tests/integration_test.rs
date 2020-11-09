use bullettrain::{Client, User, Value};

const API_KEY: &str = "MgfUaRCvvZMznuQyqjnQKt";
const TEST_FEATURE_NAME: &str = "test_feature";
const TEST_FEATURE_VALUE: &str = "sample feature value";
const TEST_USER_FEATURE_VALUE: &str = "user feature value";
const TEST_FLAG_NAME: &str = "test_flag";
const TEST_FLAG_VALUE: bool = true;
const TEST_TRAIT_NAME: &str = "test_trait";
const TEST_TRAIT_VALUE: &str = "sample trait value";
const TEST_TRAIT_NEW_VALUE: &str = "new value";
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

#[test]
fn test_get_features() {
    let features = Client::new(API_KEY).get_features().unwrap();
    assert_eq!(features.len(), 4);
    for f in features.iter() {
        assert!(f.feature.name != "");
    }
}

#[test]
fn test_get_user_features() {
    let features = Client::new(API_KEY)
        .get_user_features(&test_user())
        .unwrap();
    for f in features.iter() {
        assert!(f.feature.name != "");
    }
}

#[test]
fn test_has_value() {
    let client = Client::new(API_KEY);
    let ok = client.has_feature(TEST_FEATURE_NAME).unwrap();
    assert!(ok);

    let ok = client.has_feature(INVALID_NAME).unwrap();
    assert!(ok == false);
}

#[test]
fn test_feature_enabled() {
    let client = Client::new(API_KEY);
    let enabled = client.feature_enabled(TEST_FEATURE_NAME).unwrap();
    assert!(!enabled);
    let enabled = client.feature_enabled(TEST_FLAG_NAME).unwrap();
    assert!(enabled);
}

#[test]
fn test_get_value() {
    let client = Client::new(API_KEY);
    let val = client.get_value(TEST_FEATURE_NAME).unwrap().unwrap();
    match val {
        Value::String(v) => assert!(v == TEST_FEATURE_VALUE),
        _ => assert!(false),
    }

    let val = client.get_value("integer_feature").unwrap().unwrap();
    match val {
        Value::Int(v) => assert!(v == 200),
        _ => assert!(false),
    }

    let val = client.get_value("boolean_feature").unwrap().unwrap();
    match val {
        Value::Bool(v) => assert!(v == TEST_FLAG_VALUE),
        _ => assert!(false),
    }
}

#[test]
fn test_get_user_value() {
    let val = Client::new(API_KEY)
        .get_user_value(&test_user(), TEST_FEATURE_NAME)
        .unwrap()
        .unwrap();
    match val {
        Value::String(v) => assert!(v == TEST_USER_FEATURE_VALUE),
        _ => assert!(false),
    }
}

#[test]
fn test_get_traits() {
    let traits = Client::new(API_KEY)
        .get_traits(&test_user(), vec![])
        .unwrap();
    assert!(traits.len() == 2)
}

#[test]
fn test_get_trait() {
    let t = Client::new(API_KEY)
        .get_trait(&test_user(), TEST_TRAIT_NAME)
        .unwrap();
    assert!(t.value == TEST_TRAIT_VALUE)
}

#[test]
fn test_update_trait() {
    let client = Client::new(API_KEY);
    let mut old_trait = client
        .get_trait(&different_user(), TEST_TRAIT_NAME)
        .unwrap();

    old_trait.value = String::from(TEST_TRAIT_NEW_VALUE);
    let updated = client.update_trait(&different_user(), &old_trait).unwrap();
    assert!(TEST_TRAIT_NEW_VALUE == updated.value);

    let t = client
        .get_trait(&different_user(), TEST_TRAIT_NAME)
        .unwrap();
    assert!(TEST_TRAIT_NEW_VALUE == t.value);

    old_trait.value = String::from("old value");
    client.update_trait(&different_user(), &old_trait).unwrap();
}
