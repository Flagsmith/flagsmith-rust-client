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
struct FastlyRequestBuilder {
    backend: String,
    request: Result<http::Request, ()>,
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
