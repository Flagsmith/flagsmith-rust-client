use reqwest::header::{self, HeaderMap};
use std::{collections::HashMap, fmt::{self, format}, string, thread, time::Duration};
use std::rc::Rc;
use flagsmith_flag_engine::environments::builders::build_environment_struct;
use flagsmith_flag_engine::environments::Environment;
mod analytics;

const DEFAULT_API_URL: &str = "https://api.flagsmith.com/api/v1/";
use super::error;
pub struct FlagsmithOptions{
    pub api_url: String,
    pub custom_headers: HeaderMap,
    pub request_timeout_seconds: u64,
    pub environment_refresh_interval: u32,
    pub enable_analytics: bool

}
impl Default for FlagsmithOptions{
    fn default() -> Self{
        FlagsmithOptions{
            api_url: DEFAULT_API_URL.to_string(),
            custom_headers: header::HeaderMap::new(),
            request_timeout_seconds: 60,
            enable_analytics: false,
            environment_refresh_interval: 10
        }
    }
}
pub struct Flagsmith {
    client: reqwest::blocking::Client,
    environment_flags_url: String,
    identities_url: String,
    environment_url: String,
    options: FlagsmithOptions,
    datastore: Rc<DataStore>,
    environment: Option<Environment>

    //  environment_key: String,
// api_url: String,
//custom_headers: HashMap<String, String>,
//request_timeout_seconds: u8,
// enable_local_evaluation: bool,
//environment_refresh_interval_seconds: u32,
//retries: u8,
    //enable_analytics: bool, //TODO: Add default flag handler

}
struct DataStore{
    environment: Environment

}
impl Flagsmith {
    pub fn new(
        environment_key: String,
        flagsmith_options: FlagsmithOptions
        // api_url: Option<String>,
        // custom_headers: Option<HeaderMap>,
        // request_timeout_seconds: Option<u64>,
        // enable_local_evaluation: Option<bool>,
        // environment_refresh_interval_seconds: Option<u32>,
        // retries: Option<u8>,
        // enable_analytics: Option<bool>, // TODO: Add this default_flag_handler:

    ) -> Flagsmith {
        let mut headers = flagsmith_options.custom_headers.clone();//custom_headers.unwrap_or(header::HeaderMap::new());
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str(&environment_key).unwrap(),
        );
        let timeout = Duration::from_secs(flagsmith_options.request_timeout_seconds);
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout).build().unwrap();

        let environment_flags_url = format!("{}flags/", flagsmith_options.api_url);
        let identities_url = format!("{}identities/",flagsmith_options.api_url);
        let environment_url = format!("{}environment-document/", flagsmith_options.api_url);
        let mut flagsmith = Rc::new(Flagsmith {
            client: client,
            environment_flags_url:environment_flags_url,
            environment_url: environment_url,
            identities_url: identities_url,
            options: flagsmith_options,
            environment: None
        });
        let flagsmith = Rc::clone(&flagsmith);
        thread::spawn(move ||{
            for i in 1..10{
                println!("updating environment");
                flagsmith.update_environment();
                thread::sleep(Duration::from_secs(10));
            }
        });
        return flagsmith;
    }
    pub fn update_environment(&mut self) -> Result<(), error::Error>{
        self.environment = Some(self.get_environment_from_api()?);
        return Ok(());
    }
    fn get_environment_from_api(&self) -> Result<Environment, error::Error>{
        let method = reqwest::Method::GET;
        let url=  self.environment_url.clone();
        let json_document = self.get_json_response(method, url)?;
        let environment = build_environment_struct(json_document);
        return Ok(environment)

    }

    fn get_json_response(&self, method: reqwest::Method, url: String) -> Result<serde_json::Value, error::Error>{
        let response = self.client.request(method, url).send()?;
        if response.status().is_success(){
            return Ok(response.json()?)
        }else {
            return Err(error::Error::from("Request returned non 2xx".to_string()))
        }
    }
}
