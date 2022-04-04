use flagsmith_flag_engine::environments::builders::build_environment_struct;
use flagsmith_flag_engine::environments::Environment;
use flagsmith_flag_engine::identities::Trait;
use reqwest::header::{self, HeaderMap};
use std::sync::{Arc, Mutex};
use std::{
    collections::HashMap,
    fmt::{self, format},
    string, thread,
    time::Duration,
};
mod analytics;
mod models;
const DEFAULT_API_URL: &str = "https://api.flagsmith.com/api/v1/";
use super::error;
pub struct FlagsmithOptions {
    pub api_url: String,
    pub custom_headers: HeaderMap,
    pub request_timeout_seconds: u64,
    pub enable_local_evaluation: bool,
    pub environment_refresh_interval_seconds: u64,
    pub enable_analytics: bool,
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
    environment: Option<Environment>, //  environment_key: String,
                                      // api_url: String,
                                      //custom_headers: HashMap<String, String>,
                                      //request_timeout_seconds: u8,
                                      // enable_local_evaluation: bool,
                                      //environment_refresh_interval_seconds: u32,
                                      //retries: u8,
                                      //enable_analytics: bool, //TODO: Add default flag handler
}
struct DataStore {
    environment: Option<Environment>,
}
impl Flagsmith {
    pub fn new(
        environment_key: String,
        flagsmith_options: FlagsmithOptions, // api_url: Option<String>,
                                             // custom_headers: Option<HeaderMap>,
                                             // request_timeout_seconds: Option<u64>,
                                             // enable_local_evaluation: Option<bool>,
                                             // environment_refresh_interval_seconds: Option<u32>,
                                             // retries: Option<u8>,
                                             // enable_analytics: Option<bool>, // TODO: Add this default_flag_handler:
    ) -> Flagsmith {
        let mut headers = flagsmith_options.custom_headers.clone(); //custom_headers.unwrap_or(header::HeaderMap::new());
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str(&environment_key).unwrap(),
        );
        let timeout = Duration::from_secs(flagsmith_options.request_timeout_seconds);
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .unwrap();

        let environment_flags_url = format!("{}flags/", flagsmith_options.api_url);
        let identities_url = format!("{}identities/", flagsmith_options.api_url);
        let environment_url = format!("{}environment-document/", flagsmith_options.api_url);
        let ds = Arc::new(Mutex::new(DataStore { environment: None }));
        let flagsmith = Flagsmith {
            client: client.clone(),
            environment_flags_url,
            environment_url: environment_url.clone(),
            identities_url: identities_url,
            options: flagsmith_options,
            environment: None,
            datastore: Arc::clone(&ds),
        };
        let environment_refresh_interval_seconds = flagsmith.options.environment_refresh_interval_seconds;
        if flagsmith.options.enable_local_evaluation {
            let ds = Arc::clone(&ds);
            thread::spawn(move || loop {
                println!("updating environment From Thread");
                let environment = Some(
                    get_environment_from_api(client.clone(), environment_url.clone()).unwrap(),
                );
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
            self.client.clone(),
            self.environment_url.clone(),
        )?);
        return Ok(());
    }
    pub fn get_environment_flags(&self) -> models::Flags {
        let data = self.datastore.lock().unwrap();
        let environment = data.environment.as_ref().unwrap();
        //TODO: Add fetch from api
        // if data.environment.is_some(){
        // }

        return self.get_environment_flags_from_document(environment);
    }
    fn get_environment_flags_from_document(&self, environment: &Environment) -> models::Flags {
        return models::Flags::from_feature_states(&environment.feature_states, None);
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
    pub fn get_identity_flags(&self, identifier: String, traits: Vec<Trait>) {
    }
}
fn get_environment_from_api(
    client: reqwest::blocking::Client,
    environment_url: String,
) -> Result<Environment, error::Error> {
    let method = reqwest::Method::GET;
    let json_document = get_json_response(client, method, environment_url)?;
    let environment = build_environment_struct(json_document);
    return Ok(environment);
}
fn get_json_response(
    client: reqwest::blocking::Client,
    method: reqwest::Method,
    url: String,
) -> Result<serde_json::Value, error::Error> {
    let response = client.request(method, url).send()?;
    if response.status().is_success() {
        return Ok(response.json()?);
    } else {
        return Err(error::Error::from("Request returned non 2xx".to_string()));
    }
}
