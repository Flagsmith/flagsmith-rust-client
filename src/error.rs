use std::convert::From;
use std::error;
use std::fmt;

use reqwest::StatusCode;

/// Wraps several types of errors.
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub msg: String,
}

/// Defines error kind.
#[derive(Debug, PartialEq)]
pub enum ErrorKind {
    FlagsmithClientError,
    FlagsmithAPIError,
}
impl Error{
    pub fn new(kind: ErrorKind, msg: String) -> Error{
        Error{
            kind,
            msg
        }
    }

    pub fn http(status: StatusCode, body: String) -> Error {
        Error {
            kind: ErrorKind::FlagsmithAPIError,
            msg: format!("HTTP Api error: {status}, {body}")
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::FlagsmithClientError => write!(f, "Flagsmith client error: {}", &self.msg),
            ErrorKind::FlagsmithAPIError => write!(f, "Flagsmith API error: {}", &self.msg),
        }
    }
}

impl error::Error for Error {}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error::new(ErrorKind::FlagsmithClientError, e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::new(ErrorKind::FlagsmithAPIError, e.to_string())
    }
}

impl  From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::new(ErrorKind::FlagsmithAPIError, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_includes_status_and_body() {
        let error = Error::http(
            StatusCode::BAD_GATEWAY,
            "{\"detail\":\"upstream unavailable\"}".to_string(),
        );

        assert_eq!(error.kind, ErrorKind::FlagsmithAPIError);
        assert_eq!(
            error.msg,
            "HTTP Api error: 502 Bad Gateway, {\"detail\":\"upstream unavailable\"}"
        );
        assert_eq!(
            error.to_string(),
            "Flagsmith API error: HTTP Api error: 502 Bad Gateway, {\"detail\":\"upstream unavailable\"}"
        );
    }
}
