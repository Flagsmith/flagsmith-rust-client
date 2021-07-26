//! flagsmith create provides client for flagsmith.com API.
//!
//! # Example
//!
//! ```rust
//! # const API_KEY: &str = "MgfUaRCvvZMznuQyqjnQKt";
//! use flagsmith::{Client,Value};
//!
//! let client = Client::new(API_KEY);
//! if client.feature_enabled("test_feature")? {
//!     if let Some(Value::Int(i)) = client.get_value("integer_feature")? {
//!         println!("integer value: {}", i);
//!         # assert!(i == 200);
//!     }
//!     // ...
//! }
//! # Ok::<(), flagsmith::error::Error>(())
//! ```

pub mod error;
use serde::{Deserialize, Serialize};

/// Default address of Flagsmith API.
pub const DEFAULT_BASE_URI: &str = "https://api.flagsmith.com/api/v1/";

/// Contains core information about feature.
#[derive(Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub description: Option<String>,
}

/// Represents remote config value.
///
/// Currently there are three possible types of values: booleans, integers and strings.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    String(String),
}

/// Contains information about Feature and it's value.
#[derive(Serialize, Deserialize)]
pub struct Flag {
    pub feature: Feature,
    #[serde(rename = "feature_state_value")]
    pub state_value: Option<Value>,
    pub enabled: bool,
}

/// Holds identity information.
#[derive(Serialize, Deserialize)]
pub struct User {
    pub identifier: String,
}

/// Holds information about User's trait.
#[derive(Serialize, Deserialize)]
pub struct Trait {
    pub identity: Option<User>,
    #[serde(rename = "trait_key")]
    pub key: String,
    #[serde(rename = "trait_value")]
    pub value: String,
}

/// Provides various methods to interact with Flagsmith API.
///
/// Static method new can be used to create instance configured with default API address.
/// To use custom API address, use struct constructor.
///
/// # Example
///
/// ```rust
/// let client = flagsmith::Client {
///     api_key: String::from("secret key"),
///     base_uri: String::from("https://features.on.my.own.server/api/v1/"),
/// };
/// # match client.get_features() {
/// #    Err(e) => println!("{}", e),
/// #    Ok(f) => assert!(false),
/// # }
/// ```
pub struct Client {
    pub api_key: String,
    pub base_uri: String,
}

/// Internal structure used for deserialization.
#[derive(Serialize, Deserialize)]
struct TraitResponse {
    traits: Vec<Trait>,
}

impl Client {
    /// Returns Client instance configured to use default API address and given API key.
    pub fn new(api_key: &str) -> Client {
        return Client {
            api_key: String::from(api_key),
            base_uri: String::from(DEFAULT_BASE_URI),
        };
    }

    /// Returns all features available in given environment.
    pub fn get_features(&self) -> Result<Vec<Flag>, error::Error> {
        let resp = self
            .build_request(vec!["flags/"])?
            .send()?
            .json::<Vec<Flag>>()?;
        Ok(resp)
    }

    /// Returns all features as defined for given user.
    pub fn get_user_features(&self, user: &User) -> Result<Vec<Flag>, error::Error> {
        let resp = self
            .build_request(vec!["flags/", &user.identifier])?
            .send()?
            .json::<Vec<Flag>>()?;
        Ok(resp)
    }

    /// Returns information whether given feature is defined.
    pub fn has_feature(&self, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Returns information whether given feature is defined for given user.
    pub fn has_user_feature(&self, user: &User, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_user_features(user)?, name);
        match flag {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Returns information whether given feature flag is enabled.
    pub fn feature_enabled(&self, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(f) => Ok(f.enabled),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    /// Returns information whether given feature flag is enabled for given user.
    pub fn user_feature_enabled(&self, user: &User, name: &str) -> Result<bool, error::Error> {
        let flag = self.get_flag(self.get_user_features(user)?, name);
        match flag {
            Some(f) => Ok(f.enabled),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    /// Returns value of given feature (remote config).
    ///
    /// Returned value can have one of following types: boolean, integer, string.
    pub fn get_value(&self, name: &str) -> Result<Option<Value>, error::Error> {
        let flag = self.get_flag(self.get_features()?, name);
        match flag {
            Some(f) => Ok(f.state_value),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    /// Returns value of given feature (remote config) as defined for given user.
    ///
    /// Returned value can have one of following types: boolean, integer, string.
    pub fn get_user_value(&self, user: &User, name: &str) -> Result<Option<Value>, error::Error> {
        let flag = self.get_flag(self.get_user_features(user)?, name);
        match flag {
            Some(f) => Ok(f.state_value),
            None => Err(error::Error::from(format!("unknown feature {}", name))),
        }
    }

    /// Returns trait defined for given user.
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

    /// Returns all traits defined for given user.
    ///
    /// If keys are provided, get_traits returns only corresponding traits,
    /// otherwise all traits for given user are returned.
    pub fn get_traits(&self, user: &User, keys: Vec<&str>) -> Result<Vec<Trait>, error::Error> {
        let resp = self
            .build_request(vec!["identities/"])?
            .query(&[("identifier", &user.identifier)])
            .send()?
            .json::<TraitResponse>()?;

        let mut traits = resp.traits;
        if keys.is_empty() {
            return Ok(traits);
        }

        traits.retain(|t| {
            let tk: &String = &t.key;
            keys.iter().any(|k| tk == k)
        });

        Ok(traits)
    }

    /// Updates trait value for given user, returns updated trait.
    pub fn update_trait(&self, user: &User, to_update: &Trait) -> Result<Trait, error::Error> {
        let update = Trait {
            identity: Some(User {
                identifier: user.identifier.clone(),
            }),
            key: to_update.key.clone(),
            value: to_update.value.clone(),
        };
        let url = reqwest::Url::parse(&self.base_uri)?.join("traits/")?;
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(url)
            .header("X-Environment-Key", &self.api_key)
            .json(&update)
            .send()?
            .json::<Trait>()?;

        Ok(resp)
    }

    /// Builds get request, using API URL and API key.
    fn build_request(
        &self,
        parts: Vec<&str>,
    ) -> Result<reqwest::blocking::RequestBuilder, error::Error> {
        let mut url = reqwest::Url::parse(&self.base_uri)?;
        for p in parts {
            url = url.join(p)?;
        }
        let client = reqwest::blocking::Client::new();
        Ok(client.get(url).header("X-Environment-Key", &self.api_key))
    }

    /// Returns flag by name.
    fn get_flag(&self, features: Vec<Flag>, name: &str) -> Option<Flag> {
        for f in features {
            if f.feature.name == name {
                return Some(f);
            }
        }
        None
    }
}
