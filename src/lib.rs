mod error;

use serde::{Deserialize, Serialize};

pub struct Config {
    pub base_uri: String,
}

#[derive(Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Flag {
    pub feature: Feature,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub identifier: String,
}

#[derive(Serialize, Deserialize)]
pub struct Trait {
    pub identity: User,
    pub key: String,
    pub value: String,
}

pub struct Client {
    pub api_key: String,
    pub config: Config,
}

impl Client {
    pub fn get_features(&self) -> Result<Vec<Flag>, error::Error> {
        let resp = self
            .build_request(vec!["flags/"])?
            .send()?
            .json::<Vec<Flag>>()?;
        Ok(resp)
    }

    pub fn get_user_features(&self, user: User) -> Result<Vec<Flag>, error::Error> {
        let resp = self
            .build_request(vec!["flags/", &user.identifier])?
            .send()?
            .json::<Vec<Flag>>()?;
        Ok(resp)
    }
    
    pub fn has_feature(&self, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(f) => Ok(true),
            None => Ok(false),
        }
    }
    
    pub fn feature_enabled(&self, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(f) => Ok(f.enabled),
            None => Ok(false),
        }
    }

    pub fn user_feature_enabled(&self, user: User, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_user_features(user)?, name);
        match flag {
            Some(f) => Ok(f.enabled),
            None => Ok(false),
        }
    }

    pub fn get_value(&self, name: &str) {}
    pub fn get_user_value(&self, user: User, name: &str) {}
    /*
    fn get_trait(&self, user: User, key: String) -> Result<Trait, reqwest::Error> {
    }
    fn get_traits(&self, user: User) -> Result<Vec<Trait>, reqwest::Error> {}
    fn update_trair(&self, user: User, toUpdate: Trait) -> Result<Trait, reqwest::Error> {}
    */

    fn build_request(
        &self,
        parts: Vec<&str>,
    ) -> Result<reqwest::blocking::RequestBuilder, error::Error> {
        let mut url = reqwest::Url::parse(&self.config.base_uri)?;
        for p in parts {
            url = url.join(p)?;
        }
        let client = reqwest::blocking::Client::new();
        Ok(client.get(url).header("X-Environment-Key", &self.api_key))
    }

    fn get_flag(&self, features: Vec<Flag>, name: &str) -> Option<Flag> {
        for f in features {
            if f.feature.name == name {
                return Some(f);
            }
        }
        None
    }
}
