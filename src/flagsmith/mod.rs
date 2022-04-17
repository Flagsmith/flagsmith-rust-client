use flagsmith_flag_engine::engine;
use flagsmith_flag_engine::environments::builders::build_environment_struct;
use flagsmith_flag_engine::environments::Environment;
use flagsmith_flag_engine::identities::{Identity, Trait};
use log::debug;
use reqwest::header::{self, HeaderMap};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::{
     thread,
    time::Duration,
};
mod analytics;
pub mod models;
const DEFAULT_API_URL: &str = "https://api.flagsmith.com/api/v1/";
use self::analytics::AnalyticsProcessor;
use self::models::{Flag, Flags};
use std::sync::mpsc::{self,  Sender, TryRecvError};

use super::error;
pub struct FlagsmithOptions {
    pub api_url: String,
    pub custom_headers: HeaderMap,
    pub request_timeout_seconds: u64,
    pub enable_local_evaluation: bool,
    pub environment_refresh_interval_mills: u64,
    pub enable_analytics: bool,
    pub default_flag_handler: Option<fn(&str) -> Flag>,
}

impl Default for FlagsmithOptions {
    fn default() -> Self {
        FlagsmithOptions {
            api_url: DEFAULT_API_URL.to_string(),
            custom_headers: header::HeaderMap::new(),
            request_timeout_seconds: 60,
            enable_local_evaluation: false,
            enable_analytics: false,
            environment_refresh_interval_mills: 10*1000,
            default_flag_handler: None,
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
    _polling_thead_tx: Sender<u32>, // used for shutting down polling manager
}

struct DataStore {
    environment: Option<Environment>,
}

impl Flagsmith {
    pub fn new(environment_key: String, flagsmith_options: FlagsmithOptions) -> Self{
        let mut headers = flagsmith_options.custom_headers.clone();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str(&environment_key).unwrap(),
        );
        headers.insert("Content-Type", "application/json".parse().unwrap());
        let timeout = Duration::from_secs(flagsmith_options.request_timeout_seconds);
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers.clone())
            .timeout(timeout)
            .build()
            .unwrap();

        let environment_flags_url = format!("{}flags/", flagsmith_options.api_url);
        let identities_url = format!("{}identities/", flagsmith_options.api_url);
        let environment_url = format!("{}environment-document/", flagsmith_options.api_url);
        // Initialize analytics processor
        let analytics_processor = match flagsmith_options.enable_analytics {
            true => Some(AnalyticsProcessor::new(
                flagsmith_options.api_url.clone(),
                headers,
                timeout,
                None
            )),
            false => None,
        };
        // Put the environment model behind mutex to
        // to share it safely between threads
        let ds = Arc::new(Mutex::new(DataStore { environment: None }));
        let (tx, rx) = mpsc::channel::<u32>();
        let flagsmith = Flagsmith {
            client: client.clone(),
            environment_flags_url,
            environment_url: environment_url.clone(),
            identities_url,
            options: flagsmith_options,
            datastore: Arc::clone(&ds),
            analytics_processor,
            _polling_thead_tx:tx
        };

        // Create a thread to update environment document
        // If enabled
        let environment_refresh_interval_mills =
            flagsmith.options.environment_refresh_interval_mills;
        if flagsmith.options.enable_local_evaluation {
            let ds = Arc::clone(&ds);
            thread::spawn(move || loop {
                match rx.try_recv() {
                    Ok(_) | Err(TryRecvError::Disconnected) => {
                        debug!("shutting down polling manager");
                        break;
                    }
                    Err(TryRecvError::Empty) => {}
                }

                let environment =
                    Some(get_environment_from_api(&client, environment_url.clone()).expect("updating environment document failed"));
                let mut data = ds.lock().unwrap();
                data.environment = environment;
                thread::sleep(Duration::from_millis(environment_refresh_interval_mills));

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
        return self.get_environment_flags_from_api();
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
    pub fn get_identity_flags(&self, identifier: &str, traits: Option<Vec<Trait>>) -> Result<Flags, error::Error> {
        let data = self.datastore.lock().unwrap();
        let traits = traits.unwrap_or(vec![]);
        if data.environment.is_some(){
            let environment = data.environment.as_ref().unwrap();
            return  self.get_identity_flags_from_document(environment, identifier, traits);
        }
        return self.get_identity_flags_from_api(identifier, traits);

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
        let mut data = self.datastore.lock().unwrap();
        data.environment = Some(get_environment_from_api(
            &self.client,
            self.environment_url.clone(),
        )?);
        return Ok(());
    }

    fn get_identity_flags_from_document(&self,  environment: &Environment, identifier: &str, traits: Vec<Trait>) -> Result<Flags, error::Error>{
        let identity = self.build_identity_model(environment,identifier, traits.clone())?;
        let feature_states = engine::get_identity_feature_states(environment, &identity, Some(traits.as_ref()));
        let flags = Flags::from_feature_states(&feature_states, self.analytics_processor.clone(), self.options.default_flag_handler, Some(&identity.composite_key()));
        return Ok(flags);

    }

    fn build_identity_model(&self,environment: &Environment, identifier: &str, traits: Vec<Trait>) -> Result<Identity, error::Error> {
        let mut identity = Identity::new(identifier.to_string(), environment.api_key.clone());
        identity.identity_traits = traits;
        Ok(identity)
    }
    fn get_identity_flags_from_api(&self, identifier: &str, traits: Vec<Trait>) -> Result<Flags, error::Error>{
        let method = reqwest::Method::POST;

        let json = json!({"identifier":identifier, "traits": traits});
        let response = get_json_response(&self.client, method, self.identities_url.clone(), Some(json.to_string()))?;
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
        let api_flags =
            get_json_response(&self.client, method, self.environment_flags_url.clone(), None)?;
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

fn get_json_response(
    client: &reqwest::blocking::Client,
    method: reqwest::Method,
    url: String,
    body: Option<String>
) -> Result<serde_json::Value, error::Error> {
    let mut request = client.request(method, url);
    if body.is_some(){
        request = request.body(body.unwrap());
    };
    let response = request.send()?;
    if response.status().is_success() {
        return Ok(response.json()?);
    } else {
        return Err(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            response.text()?
        ));
    }
}

#[cfg(test)]
mod tests{
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
                "id": 1,
                "hide_disabled_flags": false,
                "segments": []
            },
            "segment_overrides": [],
            "id": 1,
            "feature_states": []
    }"#;

    #[test]
    fn polling_thread_updates_environment_on_start(){
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
            enable_local_evaluation:true,
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
    fn polling_thread_updates_environment_on_each_refresh(){
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
            enable_local_evaluation:true,
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

}
