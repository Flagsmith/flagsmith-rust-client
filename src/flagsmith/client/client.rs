use std::time::Duration;

use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;

#[cfg(not(feature = "non_blocking"))]
use crate::flagsmith::client::blocking_client::BlockingClient;
#[cfg(feature = "non_blocking")]
use crate::flagsmith::client::fastly_client::FastlyClient;

pub enum Method {
    OPTIONS,
    GET,
    POST,
    PUT,
    DELETE,
    HEAD,
    TRACE,
    CONNECT,
    PATCH,
}

pub trait ResponseStatusCode {
    fn is_success(&self) -> bool;
}

pub trait ClientRequestBuilder {
    fn with_body(self, body: String) -> Self;

    // TODO return type
    fn send(self) -> Result<impl ClientResponse, ()>;
}

pub trait ClientResponse {
    fn status(&self) -> impl ResponseStatusCode;

    // TODO return error type
    fn text(self) -> Result<String, ()>;

    // TODO return error type
    fn json<T: DeserializeOwned>(self) -> Result<T, ()>;
}

pub trait ClientLike {
    fn new(headers: HeaderMap, timeout: Duration) -> Self;
    fn request(&self, method: Method, url: String) -> impl ClientRequestBuilder;
}

#[derive(Clone)]
pub struct SafeClient {
    #[cfg(not(feature = "non_blocking"))]
    pub inner: BlockingClient,

    #[cfg(feature = "non_blocking")]
    pub inner: FastlyClient,
}

impl SafeClient {
    #[cfg(not(feature = "non_blocking"))]
    pub fn new(headers: HeaderMap, timeout: Duration) -> Self {
        Self {
            inner: BlockingClient::new(headers, timeout),
        }
    }

    #[cfg(feature = "non_blocking")]
    pub fn new(headers: HeaderMap, timeout: Duration) -> Self {
        Self {
            inner: FastlyClient::new(headers, timeout),
        }
    }
}
