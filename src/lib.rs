mod error;

use serde::{Serialize,Deserialize};


pub struct Config {
    pub base_uri: String,
}

#[derive(Serialize,Deserialize)]
pub struct Feature {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub description: Option<String>,
}

#[derive(Serialize,Deserialize)]
pub struct Flag {
    pub feature: Feature,
    pub enabled: bool,
}

#[derive(Serialize,Deserialize)]
pub struct User {
    identifier: String,
}

#[derive(Serialize,Deserialize)]
pub struct Trait {
    identity: User,
    key: String,
    value: String,
}

pub struct Client {
    pub api_key: String,
    pub config: Config,
}

impl Client {
    fn build_request(&self, parts: Vec<String>) -> Result<reqwest::blocking::RequestBuilder, error::Error> {
        let mut url = reqwest::Url::parse(&self.config.base_uri)?;
        for p in parts {
            url = url.join(&p)?;
        }
        let client = reqwest::blocking::Client::new();
        Ok(client.get(url).header("X-Environment-Key", &self.api_key))
    }

    pub fn get_features(&self) -> Result<Vec<Flag>, error::Error> {
        let resp = self.build_request(vec!["flags/".to_string()])?.send()?.json::< Vec<Flag> >()?;
        Ok(resp)
    }

    pub fn get_user_features(&self, user: User) -> Result<Vec<Flag>, error::Error> {
        let resp = self.build_request(vec!["flags/".to_string(), user.identifier])?.send()?.json::< Vec<Flag> >()?;
        Ok(resp)
    }
    pub fn has_feature(&self, name: String) -> Result<bool, error::Error> {
        Ok(false)
    }
    pub fn feature_enabled(&self, name: String) -> Result<bool, error::Error> {
        Ok(false)
    }
    pub fn user_feature_enabled(&self, name: String) -> Result<bool, error::Error> {
        Ok(false)
    }
    pub fn get_value(&self, name: String) {}
    pub fn get_user_value(&self, user: User, name: String) {}
    /*
       fn get_trait(&self, user: User, key: String) -> Result<Trait, reqwest::Error> {
       }
       fn get_traits(&self, user: User) -> Result<Vec<Trait>, reqwest::Error> {}
       fn update_trair(&self, user: User, toUpdate: Trait) -> Result<Trait, reqwest::Error> {}
       */
}
