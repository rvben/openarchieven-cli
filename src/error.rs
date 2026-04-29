//! Error type and `ErrorKind` enum, plus structured stderr emission.

use serde::Serialize;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    NotFound,
    RateLimit,
    Timeout,
    Network,
    Server,
    Parse,
    Conflict,
}

impl ErrorKind {
    pub fn retryable(self) -> bool {
        matches!(
            self,
            ErrorKind::RateLimit | ErrorKind::Timeout | ErrorKind::Network | ErrorKind::Server
        )
    }

    pub fn exit_code(self) -> u8 {
        match self {
            ErrorKind::Validation => 2,
            _ => 1,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Validation => "validation",
            ErrorKind::NotFound => "not_found",
            ErrorKind::RateLimit => "rate_limit",
            ErrorKind::Timeout => "timeout",
            ErrorKind::Network => "network",
            ErrorKind::Server => "server",
            ErrorKind::Parse => "parse",
            ErrorKind::Conflict => "conflict",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
#[error("{kind}: {message}")]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub retry_after_seconds: Option<u64>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            upstream_code: None,
            upstream_message: None,
            retry_after_seconds: None,
        }
    }

    pub fn with_upstream(mut self, code: impl Into<String>, message: impl Into<String>) -> Self {
        self.upstream_code = Some(code.into());
        self.upstream_message = Some(message.into());
        self
    }

    pub fn with_retry_after(mut self, secs: u64) -> Self {
        self.retry_after_seconds = Some(secs);
        self
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_exits_with_2() {
        assert_eq!(ErrorKind::Validation.exit_code(), 2);
    }

    #[test]
    fn other_kinds_exit_with_1() {
        for k in [
            ErrorKind::NotFound,
            ErrorKind::RateLimit,
            ErrorKind::Timeout,
            ErrorKind::Network,
            ErrorKind::Server,
            ErrorKind::Parse,
            ErrorKind::Conflict,
        ] {
            assert_eq!(k.exit_code(), 1, "{k:?}");
        }
    }

    #[test]
    fn retryable_set() {
        assert!(ErrorKind::RateLimit.retryable());
        assert!(ErrorKind::Timeout.retryable());
        assert!(ErrorKind::Network.retryable());
        assert!(ErrorKind::Server.retryable());
        assert!(!ErrorKind::Validation.retryable());
        assert!(!ErrorKind::NotFound.retryable());
        assert!(!ErrorKind::Parse.retryable());
        assert!(!ErrorKind::Conflict.retryable());
    }

    #[test]
    fn kind_string_is_snake_case() {
        assert_eq!(ErrorKind::RateLimit.as_str(), "rate_limit");
        assert_eq!(ErrorKind::NotFound.as_str(), "not_found");
    }
}
