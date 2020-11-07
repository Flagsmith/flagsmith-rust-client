use std::convert::From;
use std::fmt;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    desc: String,
}

#[derive(Debug)]
pub enum ErrorKind {
    ParseError,
    RequestError,
    AppError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::ParseError => write!(f, "URL parsing error: {}", &self.desc),
            ErrorKind::RequestError => write!(f, "REST API request error: {}", &self.desc),
            ErrorKind::AppError => write!(f, "Application error: {}", &self.desc),
        }
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error {
            kind: ErrorKind::ParseError,
            desc: e.to_string(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error {
            kind: ErrorKind::RequestError,
            desc: e.to_string(),
        }
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error {
            kind: ErrorKind::AppError,
            desc: s,
        }
    }
}
