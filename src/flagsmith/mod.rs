use self::analytics::AnalyticsProcessor;
use self::models::{Flag, Flags};
use super::error;
use flagsmith_flag_engine::engine::get_evaluation_result;
use flagsmith_flag_engine::engine_eval::{
    add_identity_to_context, environment_to_context, EngineEvaluationContext, SegmentSource,
};
use flagsmith_flag_engine::environments::builders::build_environment_struct;
use flagsmith_flag_engine::environments::Environment;
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::segments::Segment;
use log::debug;
use models::SDKTrait;
use reqwest::header::{self, HeaderMap};
use serde_json::json;
use std::collections::HashMap;
use std::sync::mpsc::{self, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::{thread, time::Duration};

mod analytics;

pub mod models;
pub mod offline_handler;

const DEFAULT_API_URL: &str = "https://edge.api.flagsmith.com/api/v1/";

// Get the SDK version from Cargo.toml at compile time, or default to "unknown"
fn get_user_agent() -> String {
    let version = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
    format!("flagsmith-rust-sdk/{}", version)
}

pub struct FlagsmithOptions {
    pub api_url: String,
    pub custom_headers: HeaderMap,
    pub request_timeout_seconds: u64,
    pub enable_local_evaluation: bool,
    pub environment_refresh_interval_mills: u64,
    pub enable_analytics: bool,
    pub default_flag_handler: Option<fn(&str) -> Flag>,
    pub offline_handler: Option<Box<dyn offline_handler::OfflineHandler + Send + Sync>>,
    pub offline_mode: bool,
}

impl Default for FlagsmithOptions {
    fn default() -> Self {
        FlagsmithOptions {
            api_url: DEFAULT_API_URL.to_string(),
            custom_headers: header::HeaderMap::new(),
            request_timeout_seconds: 10,
            enable_local_evaluation: false,
            enable_analytics: false,
            environment_refresh_interval_mills: 60 * 1000,
            default_flag_handler: None,
            offline_handler: None,
            offline_mode: false,
        }
    }
}

pub struct Flagsmith {
    client: reqwest::blocking::Client,
    environment_flags_url: String,
    identities_url: String,
    environment_url: String,
    options: FlagsmithOptions,
    datastore: Arc<Mutex<DataStore>>,
    analytics_processor: Option<AnalyticsProcessor>,
    _polling_thread_tx: SyncSender<u32>, // to trigger polling manager shutdown
}

struct DataStore {
    environment: Option<Environment>,
    evaluation_context: Option<EngineEvaluationContext>,
}

impl Flagsmith {
    pub fn new(environment_key: String, flagsmith_options: FlagsmithOptions) -> Self {
        let mut headers = flagsmith_options.custom_headers.clone();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str(&environment_key).unwrap(),
        );
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(&get_user_agent()).unwrap(),
        );
        let timeout = Duration::from_secs(flagsmith_options.request_timeout_seconds);
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers.clone())
            .timeout(timeout)
            .build()
            .unwrap();

        let environment_flags_url = format!("{}flags/", flagsmith_options.api_url);
        let identities_url = format!("{}identities/", flagsmith_options.api_url);
        let environment_url = format!("{}environment-document/", flagsmith_options.api_url);

        if flagsmith_options.offline_mode && flagsmith_options.offline_handler.is_none() {
            panic!("offline_handler must be set to use offline_mode")
        }
        if flagsmith_options.default_flag_handler.is_some()
            && flagsmith_options.offline_handler.is_some()
        {
            panic!("default_flag_handler cannot be used with offline_handler")
        }
        if flagsmith_options.enable_local_evaluation && flagsmith_options.offline_handler.is_some()
        {
            panic!("offline_handler cannot be used with local evaluation")
        }
        if flagsmith_options.enable_local_evaluation && !environment_key.starts_with("ser.") {
            panic!("In order to use local evaluation, please use a server-side environment key (starts with 'ser.')")
        }

        // Initialize analytics processor
        let analytics_processor = match flagsmith_options.enable_analytics {
            true => Some(AnalyticsProcessor::new(
                flagsmith_options.api_url.clone(),
                headers,
                timeout,
                None,
            )),
            false => None,
        };

        // Put the environment model behind mutex to
        // to share it safely between threads
        let ds = Arc::new(Mutex::new(DataStore {
            environment: None,
            evaluation_context: None,
        }));
        let (tx, rx) = mpsc::sync_channel::<u32>(1);

        let flagsmith = Flagsmith {
            client: client.clone(),
            environment_flags_url,
            environment_url: environment_url.clone(),
            identities_url,
            options: flagsmith_options,
            datastore: Arc::clone(&ds),
            analytics_processor,
            _polling_thread_tx: tx,
        };

        if flagsmith.options.offline_handler.is_some() {
            let mut data = flagsmith.datastore.lock().unwrap();
            let environment = flagsmith
                .options
                .offline_handler
                .as_ref()
                .unwrap()
                .get_environment();

            // Create evaluation context from offline environment
            let eval_context = environment_to_context(environment.clone());
            data.evaluation_context = Some(eval_context);

            data.environment = Some(environment);
        }

        // Create a thread to update environment document
        // If enabled
        let environment_refresh_interval_mills =
            flagsmith.options.environment_refresh_interval_mills;

        if flagsmith.options.enable_local_evaluation {
            // Update environment once...
            if let Err(e) = update_environment(&client, &ds, &environment_url) {
                log::warn!("Failed to fetch environment on initialization: {}. Will retry in background.", e);
            }

            // ...and continue updating in the background
            let ds = Arc::clone(&ds);
            thread::spawn(move || loop {
                match rx.try_recv() {
                    Ok(_) | Err(TryRecvError::Disconnected) => {
                        debug!("shutting down polling manager");
                        break;
                    }
                    Err(TryRecvError::Empty) => {}
                }
                thread::sleep(Duration::from_millis(environment_refresh_interval_mills));
                if let Err(e) = update_environment(&client, &ds, &environment_url) {
                    log::warn!("Failed to update environment: {}. Will retry on next interval.", e);
                }
            });
        }
        return flagsmith;
    }
    //Returns `Flags` struct holding all the flags for the current environment.
    pub fn get_environment_flags(&self) -> Result<models::Flags, error::Error> {
        let data = self.datastore.lock().unwrap();
        if data.evaluation_context.is_some() {
            let eval_context = data.evaluation_context.as_ref().unwrap();
            return Ok(self.get_environment_flags_from_document(eval_context));
        }
        return self.default_handler_if_err(self.get_environment_flags_from_api());
    }

    // Returns all the flags for the current environment for a given identity. Will also
    // upsert all traits to the Flagsmith API for future evaluations. Providing a
    // trait with a value of None will remove the trait from the identity if it exists.
    // # Example
    // ```
    // use flagsmith_flag_engine::identities::Trait;
    // use flagsmith::{Flagsmith, FlagsmithOptions};
    // use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};
    // const ENVIRONMENT_KEY: &str = "YOUR_ENVIRONMENT_KEY";
    // fn main(){
    //     let flagsmith_options = FlagsmithOptions::default();
    //     let traits = vec![Trait{trait_key:"random_key".to_string(), trait_value: FlagsmithValue{value:"10.1".to_string(), value_type:FlagsmithValueType::Float}},
    //                       Trait{trait_key:"another_random_key".to_string(), trait_value: FlagsmithValue{value:"false".to_string(), value_type:FlagsmithValueType::Bool}},
    //                       Trait{trait_key:"another_random_key".to_string(), trait_value: FlagsmithValue{value:"".to_string(), value_type:FlagsmithValueType::None}}
    //     ];
    //     let mut flagsmith = Flagsmith::new(ENVIRONMENT_KEY.to_string(), flagsmith_options);

    //     let flags = flagsmith.get_identity_flags("user_identifier".to_string(), traits);
    // }
    //```
    pub fn get_identity_flags(
        &self,
        identifier: &str,
        traits: Option<Vec<SDKTrait>>,
        transient: Option<bool>,
    ) -> Result<Flags, error::Error> {
        let data = self.datastore.lock().unwrap();
        let traits = traits.unwrap_or(vec![]);
        if data.evaluation_context.is_some() {
            let eval_context = data.evaluation_context.as_ref().unwrap();
            let engine_traits: Vec<Trait> = traits.into_iter().map(|t| t.into()).collect();
            return self.get_identity_flags_from_document(
                eval_context,
                identifier,
                engine_traits,
            );
        }
        return self.default_handler_if_err(self.get_identity_flags_from_api(
            identifier,
            traits,
            transient.unwrap_or(false),
        ));
    }
    // Returns a list of segments that the given identity is part of
    pub fn get_identity_segments(
        &self,
        identifier: &str,
        traits: Option<Vec<Trait>>,
    ) -> Result<Vec<Segment>, error::Error> {
        let data = self.datastore.lock().unwrap();
        if data.evaluation_context.is_none() {
            return Err(error::Error::new(
                error::ErrorKind::FlagsmithClientError,
                "Local evaluation required to obtain identity segments.".to_string(),
            ));
        }
        let eval_context = data.evaluation_context.as_ref().unwrap();
        let traits = traits.unwrap_or(vec![]);

        let context_with_identity = add_identity_to_context(eval_context, identifier, &traits);

        let result = get_evaluation_result(&context_with_identity);

        let segments: Vec<Segment> = result
            .segments
            .iter()
            .filter(|seg_result| {
                seg_result.metadata.source == SegmentSource::Api
            })
            .map(|seg_result| Segment {
                id: seg_result.metadata.segment_id.unwrap_or(0) as u32,
                name: seg_result.name.clone(),
                rules: vec![],
                feature_states: vec![],
            })
            .collect();

        return Ok(segments);
    }

    fn default_handler_if_err(
        &self,
        result: Result<Flags, error::Error>,
    ) -> Result<Flags, error::Error> {
        match result {
            Ok(result) => Ok(result),
            Err(e) => {
                if self.options.default_flag_handler.is_some() {
                    return Ok(Flags::from_api_flags(
                        &vec![],
                        self.analytics_processor.clone(),
                        self.options.default_flag_handler,
                    )
                    .unwrap());
                } else {
                    Err(e)
                }
            }
        }
    }
    fn get_environment_flags_from_document(&self, eval_context: &EngineEvaluationContext) -> models::Flags {
        // Clear segments and identity for environment evaluation
        let environment_eval_ctx = EngineEvaluationContext {
            environment: eval_context.environment.clone(),
            features: eval_context.features.clone(),
            segments: HashMap::new(),
            identity: None,
        };
        let result = get_evaluation_result(&environment_eval_ctx);
        return models::Flags::from_evaluation_result(
            &result,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
        );
    }
    pub fn update_environment(&mut self) -> Result<(), error::Error> {
        return update_environment(&self.client, &self.datastore, &self.environment_url);
    }

    fn get_identity_flags_from_document(
        &self,
        eval_context: &EngineEvaluationContext,
        identifier: &str,
        traits: Vec<Trait>,
    ) -> Result<Flags, error::Error> {
        let context_with_identity = add_identity_to_context(eval_context, identifier, &traits);

        let result = get_evaluation_result(&context_with_identity);

        let flags = Flags::from_evaluation_result(
            &result,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
        );
        return Ok(flags);
    }

    fn get_identity_flags_from_api(
        &self,
        identifier: &str,
        traits: Vec<SDKTrait>,
        transient: bool,
    ) -> Result<Flags, error::Error> {
        let method = reqwest::Method::POST;

        let json = json!({"identifier":identifier, "traits": traits, "transient": transient});
        let response = get_json_response(
            &self.client,
            method,
            self.identities_url.clone(),
            Some(json.to_string()),
        )?;
        // Cast to array of values
        let api_flags = response["flags"].as_array().ok_or(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            "Unable to get valid response from Flagsmith API.".to_string(),
        ))?;

        let flags = Flags::from_api_flags(
            api_flags,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
        )
        .ok_or(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            "Unable to get valid response from Flagsmith API.".to_string(),
        ))?;
        return Ok(flags);
    }
    fn get_environment_flags_from_api(&self) -> Result<Flags, error::Error> {
        let method = reqwest::Method::GET;
        let api_flags = get_json_response(
            &self.client,
            method,
            self.environment_flags_url.clone(),
            None,
        )?;
        // Cast to array of values
        let api_flags = api_flags.as_array().ok_or(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            "Unable to get valid response from Flagsmith API.".to_string(),
        ))?;

        let flags = Flags::from_api_flags(
            api_flags,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
        )
        .ok_or(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            "Unable to get valid response from Flagsmith API.".to_string(),
        ))?;
        return Ok(flags);
    }
}

fn get_environment_from_api(
    client: &reqwest::blocking::Client,
    environment_url: String,
) -> Result<Environment, error::Error> {
    let method = reqwest::Method::GET;
    let json_document = get_json_response(client, method, environment_url, None)?;
    let environment = build_environment_struct(json_document);
    return Ok(environment);
}

fn update_environment(
    client: &reqwest::blocking::Client,
    datastore: &Arc<Mutex<DataStore>>,
    environment_url: &String,
) -> Result<(), error::Error> {
    let mut data = datastore.lock().unwrap();
    let environment = Some(get_environment_from_api(
        &client,
        environment_url.clone(),
    )?);

    let eval_context = environment_to_context(environment.as_ref().unwrap().clone());
    data.evaluation_context = Some(eval_context);

    data.environment = environment;
    return Ok(());
}

fn get_json_response(
    client: &reqwest::blocking::Client,
    method: reqwest::Method,
    url: String,
    body: Option<String>,
) -> Result<serde_json::Value, error::Error> {
    let mut request = client.request(method, url);
    if body.is_some() {
        request = request.body(body.unwrap());
    };
    let response = request.send()?;
    if response.status().is_success() {
        return Ok(response.json()?);
    } else {
        return Err(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            response.text()?,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    static ENVIRONMENT_JSON: &str = r#"{
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
          "segments": [],
          "id": 1,
          "hide_disabled_flags": false
        },
        "segment_overrides": [],
        "id": 1,
        "feature_states": [
          {
            "multivariate_feature_state_values": [],
            "feature_state_value": "some-value",
            "id": 1,
            "featurestate_uuid": "40eb539d-3713-4720-bbd4-829dbef10d51",
            "feature": {
              "name": "some_feature",
              "type": "STANDARD",
              "id": 1
            },
            "segment_id": null,
            "enabled": true
          },
          {
            "feature": {
              "id": 83755,
              "name": "test_mv",
              "type": "MULTIVARIATE"
            },
            "enabled": false,
            "django_id": 482285,
            "feature_segment": null,
            "featurestate_uuid": "c3af5fbf-39ba-422c-a846-f2fea952b37c",
            "feature_state_value": "1111",
            "multivariate_feature_state_values": [
              {
                "multivariate_feature_option": {
                  "value": "8888",
                  "id": 11516
                },
                "percentage_allocation": 100.0,
                "id": 38451,
                "mv_fs_value_uuid": "a4299c73-2430-47e4-9185-42accd01686c"
              }
            ]
          }
        ],
        "updated_at": "2023-07-14 16:12:00.000000",
        "identity_overrides": [
          {
            "identifier": "overridden-id",
            "identity_uuid": "0f21cde8-63c5-4e50-baca-87897fa6cd01",
            "created_date": "2019-08-27T14:53:45.698555Z",
            "updated_at": "2023-07-14 16:12:00.000000",
            "environment_api_key": "B62qaMZNwfiqT76p38ggrQ",
            "identity_features": [
              {
                "id": 1,
                "feature": {
                  "id": 1,
                  "name": "some_feature",
                  "type": "STANDARD"
                },
                "featurestate_uuid": "1bddb9a5-7e59-42c6-9be9-625fa369749f",
                "feature_state_value": "some-overridden-value",
                "enabled": false,
                "environment": 1,
                "identity": null,
                "feature_segment": null
              }
            ]
          }
        ]
      }"#;

    #[test]
    fn client_implements_send_and_sync() {
        // Given
        fn implements_send_and_sync<T: Send + Sync>() {}
        // Then
        implements_send_and_sync::<Flagsmith>();
    }

    #[test]
    fn test_get_user_agent_format() {
        // When
        let user_agent = get_user_agent();

        // Then
        assert!(user_agent.starts_with("flagsmith-rust-sdk/"));

        // Extract version part after the slash
        let version = user_agent.strip_prefix("flagsmith-rust-sdk/").unwrap();

        // During cargo test, CARGO_PKG_VERSION is always set, so we should never get "unknown"
        assert_ne!(version, "unknown", "Version should not be 'unknown' during cargo test");

        // Version should contain numbers (semantic versioning: e.g., "2.0.0")
        assert!(
            version.chars().any(|c| c.is_numeric()),
            "Version should contain numbers, got: {}",
            version
        );
    }

    #[test]
    fn polling_thread_updates_environment_on_start() {
        // Given
        let environment_key = "ser.test_environment_key";
        let response_body: serde_json::Value = serde_json::from_str(ENVIRONMENT_JSON).unwrap();

        let mock_server = MockServer::start();
        let api_mock = mock_server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/environment-document/")
                .header("X-Environment-Key", environment_key);
            then.status(200).json_body(response_body);
        });

        let url = mock_server.url("/api/v1/");

        let flagsmith_options = FlagsmithOptions {
            api_url: url,
            enable_local_evaluation: true,
            ..Default::default()
        };
        // When
        let _flagsmith = Flagsmith::new(environment_key.to_string(), flagsmith_options);

        // let's wait for the thread to make the request
        thread::sleep(std::time::Duration::from_millis(50));
        // Then
        api_mock.assert();
    }

    #[test]
    fn polling_thread_updates_environment_on_each_refresh() {
        // Given
        let environment_key = "ser.test_environment_key";
        let response_body: serde_json::Value = serde_json::from_str(ENVIRONMENT_JSON).unwrap();

        let mock_server = MockServer::start();
        let api_mock = mock_server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/environment-document/")
                .header("X-Environment-Key", environment_key);
            then.status(200).json_body(response_body);
        });

        let url = mock_server.url("/api/v1/");

        let flagsmith_options = FlagsmithOptions {
            api_url: url,
            environment_refresh_interval_mills: 100,
            enable_local_evaluation: true,
            ..Default::default()
        };
        // When
        let _flagsmith = Flagsmith::new(environment_key.to_string(), flagsmith_options);
        thread::sleep(std::time::Duration::from_millis(250));
        // Then
        // 3 api calls to update environment should be made, one when the thread starts and 2
        // for each subsequent refresh
        api_mock.assert_hits(3);
    }

    #[test]
    fn test_local_evaluation_identity_override_evaluate_expected() {
        // Given
        let environment_key = "ser.test_environment_key";
        let response_body: serde_json::Value = serde_json::from_str(ENVIRONMENT_JSON).unwrap();

        let mock_server = MockServer::start();
        mock_server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/environment-document/")
                .header("X-Environment-Key", environment_key);
            then.status(200).json_body(response_body);
        });

        let url = mock_server.url("/api/v1/");

        let flagsmith_options = FlagsmithOptions {
            api_url: url,
            environment_refresh_interval_mills: 100,
            enable_local_evaluation: true,
            ..Default::default()
        };

        // When
        let mut _flagsmith = Flagsmith::new(environment_key.to_string(), flagsmith_options);

        // Then
        let flags = _flagsmith.get_environment_flags();
        let identity_flags = _flagsmith.get_identity_flags("overridden-id", None, None);
        assert_eq!(flags.unwrap().get_feature_value_as_string("some_feature").unwrap().to_owned(), "some-value");
        assert_eq!(identity_flags.unwrap().get_feature_value_as_string("some_feature").unwrap().to_owned(), "some-overridden-value");
    }

    #[test]
    fn test_user_agent_header_is_set() {
        // Given
        let environment_key = "ser.test_environment_key";
        let response_body: serde_json::Value = serde_json::from_str(ENVIRONMENT_JSON).unwrap();
        let expected_user_agent = get_user_agent();

        let mock_server = MockServer::start();
        let api_mock = mock_server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/environment-document/")
                .header("X-Environment-Key", environment_key)
                .header("User-Agent", expected_user_agent.as_str());
            then.status(200).json_body(response_body);
        });

        let url = mock_server.url("/api/v1/");

        let flagsmith_options = FlagsmithOptions {
            api_url: url,
            enable_local_evaluation: true,
            ..Default::default()
        };

        // When
        let _flagsmith = Flagsmith::new(environment_key.to_string(), flagsmith_options);

        // let's wait for the thread to make the request
        thread::sleep(std::time::Duration::from_millis(50));

        // Then
        api_mock.assert();
    }
}
