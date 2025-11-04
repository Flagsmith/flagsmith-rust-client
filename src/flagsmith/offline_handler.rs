use flagsmith_flag_engine::environments::Environment;
use std::fs;

pub trait OfflineHandler {
    fn get_environment(&self) -> Environment;
}

pub struct LocalFileHandler {
    environment: Environment,
}

impl LocalFileHandler {
    pub fn new(environment_document_path: &str) -> Result<Self, std::io::Error> {
        // Read the environment document from the specified path
        let environment_document = fs::read(environment_document_path)?;

        // Deserialize the JSON into EnvironmentModel
        let environment: Environment = serde_json::from_slice(&environment_document)?;

        // Create and initialize the LocalFileHandler
        let handler = LocalFileHandler { environment };

        Ok(handler)
    }
}

impl OfflineHandler for LocalFileHandler {
    fn get_environment(&self) -> Environment {
        self.environment.clone()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn test_local_file_handler() {
        let handler = LocalFileHandler::new("tests/fixtures/environment.json").unwrap();

        let environment = handler.get_environment();
        assert_eq!(environment.api_key, "B62qaMZNwfiqT76p38ggrQ");
    }
}
