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
#[serde(untagged)]
pub enum Value {
    String(String),
    Int(i64),
    Bool(bool),
}

#[derive(Serialize, Deserialize)]
pub struct Flag {
    pub feature: Feature,
    #[serde(rename = "feature_state_value")]
    pub state_value: Option<Value>,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub identifier: String,
}

#[derive(Serialize, Deserialize)]
pub struct Trait {
    pub identity: Option<User>,
    #[serde(rename = "trait_key")]
    pub key: String,
    #[serde(rename = "trait_value")]
    pub value: String,
}

#[derive(Serialize, Deserialize)]
struct TraitResponse {
    traits: Vec<Trait>,
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

    pub fn get_user_features(&self, user: &User) -> Result<Vec<Flag>, error::Error> {
        let resp = self
            .build_request(vec!["flags/", &user.identifier])?
            .send()?
            .json::<Vec<Flag>>()?;
        Ok(resp)
    }

    pub fn has_feature(&self, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(_) => Ok(true),
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

    pub fn user_feature_enabled(&self, user: &User, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_user_features(user)?, name);
        match flag {
            Some(f) => Ok(f.enabled),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    pub fn get_value(&self, name: &str) -> Result<Option<Value>, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(f) => Ok(f.state_value),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    pub fn get_user_value(&self, user: &User, name: &str) -> Result<Option<Value>, error::Error> {
        let flag = self.get_flag(self.get_user_features(user)?, name);
        match flag {
            Some(f) => Ok(f.state_value),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    pub fn get_trait(&self, user: &User, key: &str) -> Result<Trait, error::Error> {
        let mut traits = self.get_traits(user, vec![key])?;
        match traits.len() {
            1 => Ok(traits.remove(0)),
            _ => Err(error::Error::from(format!(
                "unknown trait {} for user {}",
                key, &user.identifier
            ))),
        }
    }

    pub fn get_traits(&self, user: &User, keys: Vec<&str>) -> Result<Vec<Trait>, error::Error> {
        let resp = self
            .build_request(vec!["identities/"])?
            .query(&[("identifier", &user.identifier)])
            .send()?
            .json::<TraitResponse>()?;

        let mut traits = resp.traits;
        if keys.len() == 0 {
            return Ok(traits);
        }

        traits.retain(|t| {
            let tk: &String = &t.key;
            keys.iter().any(|k| tk == k)
        });

        Ok(traits)
    }

    pub fn update_trair(&self, user: &User, to_update: Trait) -> Result<Trait, error::Error> {
        Err(error::Error::from(String::from("not implemented!")))
    }

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
