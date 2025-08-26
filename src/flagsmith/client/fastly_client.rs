use std::io::Read;

use reqwest::header::HeaderMap;

use crate::flagsmith::{
    self,
    client::client::{
        ClientLike, ClientRequestBuilder, ClientResponse, Method, ResponseStatusCode,
    },
};
use fastly::http;

impl From<Method> for http::Method {
    fn from(value: Method) -> Self {
        match value {
            Method::OPTIONS => http::Method::OPTIONS,
            Method::GET => http::Method::GET,
            Method::POST => http::Method::POST,
            Method::PUT => http::Method::PUT,
            Method::DELETE => http::Method::DELETE,
            Method::HEAD => http::Method::HEAD,
            Method::TRACE => http::Method::TRACE,
            Method::CONNECT => http::Method::CONNECT,
            Method::PATCH => http::Method::PATCH,
        }
    }
}

impl super::client::ResponseStatusCode for http::StatusCode {
    fn is_success(&self) -> bool {
        let raw = self.as_u16();

        raw >= 200 && raw <= 299
    }

    fn as_u16(&self) -> u16 {
        self.as_u16()
    }
}

impl super::client::ClientResponse for http::Response {
    fn status(&self) -> impl ResponseStatusCode {
        self.get_status()
    }

    fn text(mut self) -> Result<String, ()> {
        let mut buf = String::new();
        if self.get_body_mut().read_to_string(&mut buf).is_ok() {
            Ok(buf)
        } else {
            Err(())
        }
    }

    fn json<T: serde::de::DeserializeOwned>(mut self) -> Result<T, ()> {
        match self.take_body_json::<T>() {
            Ok(res) => Ok(res),
            Err(_) => Err(()),
        }
    }
}

/// Wrapper to help with abstraction of the client interface.
pub struct FastlyRequestBuilder {
    backend: String,
    pub request: Result<http::Request, ()>,
}

impl ClientRequestBuilder for FastlyRequestBuilder {
    fn with_body(mut self, body: String) -> Self {
        if let Ok(ref mut req) = self.request {
            req.set_body(body);
        }

        self
    }

    fn send(self) -> Result<impl ClientResponse, ()> {
        if let Ok(req) = self.request {
            match req.send(self.backend) {
                Ok(res) => Ok(res),
                Err(_) => Err(()),
            }
        } else {
            Err(())
        }
    }
}

#[derive(Clone)]
pub struct FastlyClient {
    default_headers: HeaderMap,
}

impl ClientLike for FastlyClient {
    fn new(headers: HeaderMap, _timeout: std::time::Duration) -> Self {
        Self {
            default_headers: headers,
        }
    }

    fn request(&self, method: super::client::Method, url: String) -> impl ClientRequestBuilder {
        let mut req = http::Request::new(
            <flagsmith::client::client::Method as Into<http::Method>>::into(method),
            url,
        );

        for (name, value) in &self.default_headers {
            if let Ok(header_val) = value.to_str() {
                req.append_header(name.to_string(), header_val);
            }
        }

        FastlyRequestBuilder {
            backend: "flagsmith".to_string(),
            request: Ok(req),
        }
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use fastly::Response;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestData {
        key: String,
    }

    #[test]
    fn test_status_code_is_success() {
        let status = http::StatusCode::from_u16(199).unwrap();
        assert!(!status.is_success());

        let status = http::StatusCode::from_u16(300).unwrap();
        assert!(!status.is_success());

        for i in 200..=299 {
            let status = http::StatusCode::from_u16(i).unwrap();

            assert!(status.is_success(), "{} should be success", i);
        }
    }

    #[test]
    fn test_response_status_returns_status() {
        let resp = Response::from_status(418);

        assert_eq!(resp.status().as_u16(), 418);
    }

    #[test]
    fn test_response_text_returns_body() {
        let resp = Response::from_body("This is a test body.");

        let text = resp.text();

        assert!(text.is_ok());
        assert_eq!(text.unwrap(), "This is a test body.");
    }

    #[test]
    fn test_response_json_returns_body() {
        let resp = Response::from_body(json!({ "key": "value" }).to_string());

        let result = resp.json::<TestData>();

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            TestData {
                key: "value".to_string()
            }
        );
    }
}
