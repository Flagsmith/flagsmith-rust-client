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
        pub fn get_features(&self) -> Result<Vec<Flag>, reqwest::Error> {
            // TODO(tzdybal): get rid of unwraps, introduce own error (enum?)
            let base = reqwest::Url::parse(&self.config.base_uri).unwrap();
            let url = base.join("flags/").unwrap();
            let client = reqwest::blocking::Client::new();
            client.get(url).header("X-Environment-Key", &self.api_key).send()?.json::< Vec<Flag> >()
        }
        fn get_user_features(&self, user: User) -> Result<Vec<Flag>, reqwest::Error> {
            let base = reqwest::Url::parse(&self.config.base_uri).unwrap();
            let url = base.join("flags/").unwrap().join(&user.identifier).unwrap();
            return reqwest::blocking::get(url)?.json::< Vec<Flag> >();
        }
        fn has_feature(&self, name: String) -> Result<bool, reqwest::Error> {
            Ok(false)
        }
        fn feature_enabled(&self, name: String) -> Result<bool, reqwest::Error> {
            Ok(false)
        }
        fn user_feature_enabled(&self, name: String) -> Result<bool, reqwest::Error> {
            Ok(false)
        }
        fn get_value(&self, name: String) {}
        fn get_user_value(&self, user: User, name: String) {}
        /*
        fn get_trait(&self, user: User, key: String) -> Result<Trait, reqwest::Error> {
        }
        fn get_traits(&self, user: User) -> Result<Vec<Trait>, reqwest::Error> {}
        fn update_trair(&self, user: User, toUpdate: Trait) -> Result<Trait, reqwest::Error> {}
        */
    }
