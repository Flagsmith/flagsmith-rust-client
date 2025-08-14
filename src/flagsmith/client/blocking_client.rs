use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::flagsmith::client::client::Method;
use crate::flagsmith::client::client::{
    ClientLike, ClientRequestBuilder, ClientResponse, ResponseStatusCode,
};

impl From<Method> for reqwest::Method {
    fn from(value: Method) -> Self {
        match value {
            Method::OPTIONS => reqwest::Method::OPTIONS,
            Method::GET => reqwest::Method::GET,
            Method::POST => reqwest::Method::POST,
            Method::PUT => reqwest::Method::PUT,
            Method::DELETE => reqwest::Method::DELETE,
            Method::HEAD => reqwest::Method::HEAD,
            Method::TRACE => reqwest::Method::TRACE,
            Method::CONNECT => reqwest::Method::CONNECT,
            Method::PATCH => reqwest::Method::PATCH,
        }
    }
}

#[derive(Clone)]
pub struct BlockingClient {
    reqwest_client: reqwest::blocking::Client,
}

impl ResponseStatusCode for reqwest::StatusCode {
    fn is_success(&self) -> bool {
        self.is_success()
    }

    fn as_u16(&self) -> u16 {
        self.as_u16()
    }
}

impl ClientResponse for reqwest::blocking::Response {
    fn status(&self) -> impl ResponseStatusCode {
        self.status()
    }

    fn text(self) -> Result<String, ()> {
        match self.text() {
            Ok(res) => Ok(res),
            Err(_) => Err(()),
        }
    }

    fn json<T: DeserializeOwned>(self) -> Result<T, ()> {
        match self.json() {
            Ok(res) => Ok(res),
            Err(_) => Err(()),
        }
    }
}

impl ClientRequestBuilder for reqwest::blocking::RequestBuilder {
    fn with_body(self, body: String) -> Self {
        self.body(body)
    }

    fn send(self) -> Result<impl ClientResponse, ()> {
        match self.send() {
            Ok(res) => Ok(res),
            Err(_) => Err(()),
        }
    }
}

impl ClientLike for BlockingClient {
    fn request(&self, method: super::client::Method, url: String) -> impl ClientRequestBuilder {
        self.reqwest_client.request(method.into(), url)
    }

    fn new(headers: reqwest::header::HeaderMap, timeout: Duration) -> Self {
        let inner = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .unwrap();

        Self {
            reqwest_client: inner,
        }
    }
}
