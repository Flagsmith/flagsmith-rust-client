use crate::flagsmith::client::client::{
    ClientLike, ClientRequestBuilder, ClientResponse, Method, ResponseStatusCode, SafeClient,
};

use self::analytics::AnalyticsProcessor;
use self::models::{Flag, Flags};
use super::error;
use flagsmith_flag_engine::engine;
use flagsmith_flag_engine::environments::builders::build_environment_struct;
use flagsmith_flag_engine::environments::Environment;
use flagsmith_flag_engine::identities::{Identity, Trait};
use flagsmith_flag_engine::segments::evaluator::get_identity_segments;
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
mod client;

pub mod models;
pub mod offline_handler;

const DEFAULT_API_URL: &str = "https://edge.api.flagsmith.com/api/v1/";

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
    client: SafeClient,
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
    identities_with_overrides_by_identifier: HashMap<String, Identity>,
}

impl Flagsmith {
    pub fn new(environment_key: String, flagsmith_options: FlagsmithOptions) -> Self {
        let mut headers = flagsmith_options.custom_headers.clone();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str(&environment_key).unwrap(),
        );
        headers.insert("Content-Type", "application/json".parse().unwrap());
        let timeout = Duration::from_secs(flagsmith_options.request_timeout_seconds);
        let client = SafeClient::new(headers.clone(), timeout);

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
            identities_with_overrides_by_identifier: HashMap::new(),
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
            data.environment = Some(
                flagsmith
                    .options
                    .offline_handler
                    .as_ref()
                    .unwrap()
                    .get_environment(),
            )
        }

        // Create a thread to update environment document
        // If enabled
        let environment_refresh_interval_mills =
            flagsmith.options.environment_refresh_interval_mills;

        if flagsmith.options.enable_local_evaluation {
            // Update environment once...
            update_environment(&client, &ds, &environment_url).unwrap();

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
                update_environment(&client, &ds, &environment_url).unwrap();
            });
        }
        return flagsmith;
    }
    //Returns `Flags` struct holding all the flags for the current environment.
    pub fn get_environment_flags(&self) -> Result<models::Flags, error::Error> {
        let data = self.datastore.lock().unwrap();
        if data.environment.is_some() {
            let environment = data.environment.as_ref().unwrap();
            return Ok(self.get_environment_flags_from_document(environment));
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
        if data.environment.is_some() {
            let environment = data.environment.as_ref().unwrap();
            let engine_traits: Vec<Trait> = traits.into_iter().map(|t| t.into()).collect();
            return self.get_identity_flags_from_document(
                environment,
                &data.identities_with_overrides_by_identifier,
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
        if data.environment.is_none() {
            return Err(error::Error::new(
                error::ErrorKind::FlagsmithClientError,
                "Local evaluation required to obtain identity segments.".to_string(),
            ));
        }
        let environment = data.environment.as_ref().unwrap();
        let identities_with_overrides_by_identifier = &data.identities_with_overrides_by_identifier;
        let identity_model = self.get_identity_model(
            &environment,
            &identities_with_overrides_by_identifier,
            identifier,
            traits.clone().unwrap_or(vec![]),
        )?;
        let segments = get_identity_segments(environment, &identity_model, traits.as_ref());
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
    fn get_environment_flags_from_document(&self, environment: &Environment) -> models::Flags {
        return models::Flags::from_feature_states(
            &environment.feature_states,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
            None,
        );
    }
    pub fn update_environment(&mut self) -> Result<(), error::Error> {
        return update_environment(&self.client, &self.datastore, &self.environment_url);
    }

    fn get_identity_flags_from_document(
        &self,
        environment: &Environment,
        identities_with_overrides_by_identifier: &HashMap<String, Identity>,
        identifier: &str,
        traits: Vec<Trait>,
    ) -> Result<Flags, error::Error> {
        let identity = self.get_identity_model(
            environment,
            identities_with_overrides_by_identifier,
            identifier,
            traits.clone(),
        )?;
        let feature_states =
            engine::get_identity_feature_states(environment, &identity, Some(traits.as_ref()));
        let flags = Flags::from_feature_states(
            &feature_states,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
            Some(&identity.composite_key()),
        );
        return Ok(flags);
    }

    fn get_identity_model(
        &self,
        environment: &Environment,
        identities_with_overrides_by_identifier: &HashMap<String, Identity>,
        identifier: &str,
        traits: Vec<Trait>,
    ) -> Result<Identity, error::Error> {
        let mut identity: Identity;

        if identities_with_overrides_by_identifier.contains_key(identifier) {
            identity = identities_with_overrides_by_identifier
                .get(identifier)
                .unwrap()
                .clone();
        } else {
            identity = Identity::new(identifier.to_string(), environment.api_key.clone());
        }

        identity.identity_traits = traits;
        return Ok(identity.to_owned());
    }
    fn get_identity_flags_from_api(
        &self,
        identifier: &str,
        traits: Vec<SDKTrait>,
        transient: bool,
    ) -> Result<Flags, error::Error> {
        let method = Method::POST;

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
        let method = Method::GET;
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
    client: &SafeClient,
    environment_url: String,
) -> Result<Environment, error::Error> {
    let method = Method::GET;
    let json_document = get_json_response(client, method, environment_url, None)?;
    let environment = build_environment_struct(json_document);
    return Ok(environment);
}

fn update_environment(
    client: &SafeClient,
    datastore: &Arc<Mutex<DataStore>>,
    environment_url: &String,
) -> Result<(), error::Error> {
    let mut data = datastore.lock().unwrap();
    let environment = Some(get_environment_from_api(&client, environment_url.clone())?);
    for identity in &environment.as_ref().unwrap().identity_overrides {
        data.identities_with_overrides_by_identifier
            .insert(identity.identifier.clone(), identity.clone());
    }
    data.environment = environment;
    return Ok(());
}

fn get_json_response(
    client: &SafeClient,
    method: Method,
    url: String,
    body: Option<String>,
) -> Result<serde_json::Value, error::Error> {
    let mut request = client.inner.request(method, url);
    if body.is_some() {
        request = request.with_body(body.unwrap());
    };
    let response = request.send().unwrap();
    if response.status().is_success() {
        return Ok(response.json().unwrap());
    } else {
        return Err(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            response.text().unwrap(),
        ));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
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
        assert_eq!(
            flags
                .unwrap()
                .get_feature_value_as_string("some_feature")
                .unwrap()
                .to_owned(),
            "some-value"
        );
        assert_eq!(
            identity_flags
                .unwrap()
                .get_feature_value_as_string("some_feature")
                .unwrap()
                .to_owned(),
            "some-overridden-value"
        );
    }
}
