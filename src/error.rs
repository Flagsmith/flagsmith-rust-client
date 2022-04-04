use std::convert::From;
use std::fmt;

/// Wraps several types of errors.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    msg: String,
}

/// Defines error kind.
#[derive(Debug)]
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
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::FlagsmithClientError => write!(f, "Flagsmith API error: {}", &self.msg),
            ErrorKind::FlagsmithAPIError => write!(f, "Flagsmith client error: {}", &self.msg),
        }
    }
}

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
