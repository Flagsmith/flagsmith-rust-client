//! Benchmarks a locally evaluating SDK client against environment documents tiered by size.

use divan::Bencher;
use flagsmith::flagsmith::models::SDKTrait;
use flagsmith::{Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};
use httpmock::prelude::*;
use std::path::PathBuf;

const ENVIRONMENTS: [&str; 3] = ["small", "medium", "large"];
const IDENTITY_OVERRIDES_IDENTIFIER: &str = "identity-override-1";
const IDENTIFIER: &str = "user-1";

fn main() {
    divan::main();
}

/// A locally evaluating client serving the environment document from memory.
struct LocalEvaluationClient {
    client: Flagsmith,
    _api: MockServer,
}

impl LocalEvaluationClient {
    fn new(environment: &str) -> Self {
        let environments = PathBuf::from(
            std::env::var("SDK_BENCHMARKS_ENVIRONMENTS")
                .expect("SDK_BENCHMARKS_ENVIRONMENTS must be set, see `make benchmark`"),
        );
        let environment_document =
            std::fs::read_to_string(environments.join(format!("{environment}.json"))).unwrap();
        let api = MockServer::start();
        api.mock(|when, then| {
            when.method(GET).path("/api/v1/environment-document/");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(environment_document);
        });
        let client = Flagsmith::new(
            "ser.benchmark".to_string(),
            FlagsmithOptions {
                api_url: api.url("/api/v1/"),
                enable_local_evaluation: true,
                environment_refresh_interval_mills: 86_400_000, // Refreshed by benchmarks only
                ..Default::default()
            },
        );
        Self { client, _api: api }
    }
}

fn traits() -> Vec<SDKTrait> {
    let value = |value: &str, value_type| FlagsmithValue {
        value: value.to_string(),
        value_type,
    };
    [
        ("plan", value("plan-7", FlagsmithValueType::String)),
        ("age", value("30", FlagsmithValueType::Integer)),
        ("country", value("GB", FlagsmithValueType::String)),
        ("version", value("2.5.1", FlagsmithValueType::String)),
        (
            "email",
            value("jane@example.com", FlagsmithValueType::String),
        ),
        ("user_id", value("12", FlagsmithValueType::Integer)),
        ("beta", value("true", FlagsmithValueType::Bool)),
        ("score", value("95", FlagsmithValueType::Integer)),
        ("tier", value("pro", FlagsmithValueType::String)),
        ("device", value("iPhone 15", FlagsmithValueType::String)),
    ]
    .into_iter()
    .map(|(key, value)| SDKTrait::new(key.to_string(), value))
    .collect()
}

/// Get a feature value from the environment flags.
#[divan::bench(args = ENVIRONMENTS)]
fn get_environment_flags(bencher: Bencher, environment: &str) {
    let sdk = LocalEvaluationClient::new(environment);
    bencher.bench_local(|| {
        sdk.client
            .get_environment_flags()
            .unwrap()
            .get_feature_value_as_string("feature_1")
            .unwrap()
    });
}

/// Check a feature for an identity with identity overrides.
#[divan::bench(args = ENVIRONMENTS)]
fn get_identity_flags_with_identity_overrides(bencher: Bencher, environment: &str) {
    let sdk = LocalEvaluationClient::new(environment);
    bencher.bench_local(|| {
        sdk.client
            .get_identity_flags(IDENTITY_OVERRIDES_IDENTIFIER, None, None)
            .unwrap()
            .is_feature_enabled("feature_8")
            .unwrap()
    });
}

/// Get a feature value for an identity without traits.
#[divan::bench(args = ENVIRONMENTS)]
fn get_identity_flags_no_traits(bencher: Bencher, environment: &str) {
    let sdk = LocalEvaluationClient::new(environment);
    bencher.bench_local(|| {
        sdk.client
            .get_identity_flags(IDENTIFIER, None, None)
            .unwrap()
            .get_feature_value_as_string("feature_1")
            .unwrap()
    });
}

/// Get a feature value for an identity with traits.
#[divan::bench(args = ENVIRONMENTS)]
fn get_identity_flags_with_traits(bencher: Bencher, environment: &str) {
    let sdk = LocalEvaluationClient::new(environment);
    bencher.with_inputs(traits).bench_local_values(|traits| {
        sdk.client
            .get_identity_flags(IDENTIFIER, Some(traits), None)
            .unwrap()
            .get_feature_value_as_string("feature_1")
            .unwrap()
    });
}

/// List the segments of an identity with traits.
#[divan::bench(args = ENVIRONMENTS)]
fn get_identity_segments_with_traits(bencher: Bencher, environment: &str) {
    let sdk = LocalEvaluationClient::new(environment);
    bencher
        .with_inputs(|| traits().into_iter().map(Trait::from).collect())
        .bench_local_values(|traits| {
            sdk.client
                .get_identity_segments(IDENTIFIER, Some(traits))
                .unwrap()
        });
}
