use flagsmith_flag_engine::engine;
use flagsmith_flag_engine::environments::builders::build_environment_struct;
use flagsmith_flag_engine::environments::Environment;
use flagsmith_flag_engine::identities::{Identity, Trait};
use log::debug;
use reqwest::header::{self, HeaderMap};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::{
    collections::HashMap,
    fmt::{self, format},
    string, thread,
    time::Duration,
};
mod analytics;
pub mod models;
const DEFAULT_API_URL: &str = "https://api.flagsmith.com/api/v1/";
use self::analytics::AnalyticsProcessor;
use self::models::{Flag, Flags};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use super::error;
pub struct FlagsmithOptions {
    pub api_url: String,
    pub custom_headers: HeaderMap,
    pub request_timeout_seconds: u64,
    pub enable_local_evaluation: bool,
    pub environment_refresh_interval_seconds: u64,
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
            environment_refresh_interval_seconds: 10,
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
    polling_thead_tx: Sender<u32>, // used for shutting down polling manager
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
            polling_thead_tx:tx
        };

        // Create a thread to update environment document
        // If enabled
        let environment_refresh_interval_seconds =
            flagsmith.options.environment_refresh_interval_seconds;
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
                    Some(get_environment_from_api(&client, environment_url.clone()).unwrap());
                let mut data = ds.lock().unwrap();
                data.environment = environment;
                thread::sleep(Duration::from_secs(environment_refresh_interval_seconds));
            });
        }
        return flagsmith;
    }
    pub fn update_environment(&mut self) -> Result<(), error::Error> {
        println!("Updating environment from main thread");
        let mut data = self.datastore.lock().unwrap();
        data.environment = Some(get_environment_from_api(
            &self.client,
            self.environment_url.clone(),
        )?);
        return Ok(());
    }
    pub fn get_environment_flags(&self) -> Result<models::Flags, error::Error> {
        let data = self.datastore.lock().unwrap();
        if data.environment.is_some() {
            let environment = data.environment.as_ref().unwrap();
            return Ok(self.get_environment_flags_from_document(environment));
        }
        return self.get_environment_flags_from_api();
    }
    fn get_environment_flags_from_document(&self, environment: &Environment) -> models::Flags {
        return models::Flags::from_feature_states(
            &environment.feature_states,
            self.analytics_processor.clone(),
            self.options.default_flag_handler,
            None,
        );
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
    pub fn get_identity_flags(&self, identifier: String, traits: Option<Vec<Trait>>) -> Result<Flags, error::Error> {
        let data = self.datastore.lock().unwrap();
        let traits = traits.unwrap_or(vec![]);
        if data.environment.is_some(){
            let environment = data.environment.as_ref().unwrap();
            return  self.get_identity_flags_from_document(environment, identifier, traits);
        }
        return self.get_identity_flags_from_api(identifier, traits);

    }

    fn get_identity_flags_from_document(&self,  environment: &Environment, identifier: String, traits: Vec<Trait>) -> Result<Flags, error::Error>{
        let identity = self.build_identity_model(environment,identifier, traits.clone())?;
        let feature_states = engine::get_identity_feature_states(environment, &identity, Some(traits.as_ref()));
        let flags = Flags::from_feature_states(&feature_states, self.analytics_processor.clone(), self.options.default_flag_handler, Some(&identity.composite_key()));
        return Ok(flags);

    }

    fn build_identity_model(&self,environment: &Environment, identifier: String, traits: Vec<Trait>) -> Result<Identity, error::Error> {
        let mut identity = Identity::new(identifier, environment.api_key.clone());
        identity.identity_traits = traits;
        Ok(identity)
    }
    fn get_identity_flags_from_api(&self, identifier: String, traits: Vec<Trait>) -> Result<Flags, error::Error>{
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
        println!("Message from the top ");
        return Err(error::Error::new(
            error::ErrorKind::FlagsmithAPIError,
            response.text()?
            //"Request returned non 2xx".to_string(),
        ));
    }
}
