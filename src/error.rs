use std::fmt;  
use std::error::Error as StdError;
use std::convert::From;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    desc: String,
}

#[derive(Debug)]
pub enum ErrorKind {
    ParseError,
    RequestError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::ParseError => f.write_str("URL parsing error: "),
            ErrorKind::RequestError => f.write_str("REST API request error: "),
        };
        f.write_str(&self.desc)
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
       return &self.desc;
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error{
            kind: ErrorKind::ParseError,
            desc: e.to_string(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error{
            kind: ErrorKind::RequestError,
            desc: e.to_string(),
        }
    }
}
