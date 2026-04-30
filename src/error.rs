//! Error type and `ErrorKind` enum, plus structured stderr emission.

use serde::Serialize;
use std::fmt;
use std::io::Write;
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
    pub(crate) kind: ErrorKind,
    pub(crate) message: String,
    pub(crate) upstream_code: Option<String>,
    pub(crate) upstream_message: Option<String>,
    pub(crate) retry_after_seconds: Option<u64>,
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

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn upstream_code(&self) -> Option<&str> {
        self.upstream_code.as_deref()
    }

    pub fn upstream_message(&self) -> Option<&str> {
        self.upstream_message.as_deref()
    }

    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.retry_after_seconds
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn is_retryable_transport(&self) -> bool {
        self.kind.retryable()
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Emit the error as a single JSON line to the given writer.
///
/// Always emits valid JSON; the canonical contract for agents.
pub fn emit_json<W: Write>(w: &mut W, err: &Error) -> std::io::Result<()> {
    #[derive(Serialize)]
    struct Payload<'a> {
        error: Body<'a>,
    }
    #[derive(Serialize)]
    struct Body<'a> {
        kind: &'static str,
        message: &'a str,
        retryable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        upstream_code: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        upstream_message: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_after_seconds: Option<u64>,
    }
    let payload = Payload {
        error: Body {
            kind: err.kind.as_str(),
            message: &err.message,
            retryable: err.kind.retryable(),
            upstream_code: err.upstream_code.as_deref(),
            upstream_message: err.upstream_message.as_deref(),
            retry_after_seconds: err.retry_after_seconds,
        },
    };
    serde_json::to_writer(&mut *w, &payload).map_err(std::io::Error::other)?;
    writeln!(w)
}

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

    #[test]
    fn emits_minimal_payload() {
        let err = Error::new(ErrorKind::NotFound, "no such record");
        let mut buf = Vec::new();
        emit_json(&mut buf, &err).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["error"]["kind"], "not_found");
        assert_eq!(v["error"]["message"], "no such record");
        assert_eq!(v["error"]["retryable"], false);
        assert!(v["error"].get("upstream_code").is_none());
        assert!(v["error"].get("retry_after_seconds").is_none());
    }

    #[test]
    fn emits_upstream_metadata() {
        let err = Error::new(ErrorKind::Validation, "invalid eventyear")
            .with_upstream("INVALID_PARAM", "eventyear must be 1500..1960");
        let mut buf = Vec::new();
        emit_json(&mut buf, &err).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["error"]["upstream_code"], "INVALID_PARAM");
        assert_eq!(
            v["error"]["upstream_message"],
            "eventyear must be 1500..1960"
        );
        assert_eq!(v["error"]["retryable"], false);
    }

    #[test]
    fn emits_retry_after() {
        let err = Error::new(ErrorKind::RateLimit, "throttled").with_retry_after(30);
        let mut buf = Vec::new();
        emit_json(&mut buf, &err).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["error"]["kind"], "rate_limit");
        assert_eq!(v["error"]["retryable"], true);
        assert_eq!(v["error"]["retry_after_seconds"], 30);
    }

    #[test]
    fn error_kind_display_uses_as_str() {
        assert_eq!(format!("{}", ErrorKind::Validation), "validation");
        assert_eq!(format!("{}", ErrorKind::NotFound), "not_found");
        assert_eq!(format!("{}", ErrorKind::RateLimit), "rate_limit");
        assert_eq!(format!("{}", ErrorKind::Timeout), "timeout");
        assert_eq!(format!("{}", ErrorKind::Network), "network");
        assert_eq!(format!("{}", ErrorKind::Server), "server");
        assert_eq!(format!("{}", ErrorKind::Parse), "parse");
        assert_eq!(format!("{}", ErrorKind::Conflict), "conflict");
    }

    #[test]
    fn error_display_includes_kind_and_message() {
        let err = Error::new(ErrorKind::NotFound, "missing record");
        let s = format!("{err}");
        assert!(s.contains("not_found"), "display: {s}");
        assert!(s.contains("missing record"), "display: {s}");
    }

    #[test]
    fn error_accessors_return_correct_values() {
        let err = Error::new(ErrorKind::Server, "boom")
            .with_upstream("ERR_CODE", "upstream msg")
            .with_retry_after(60);
        assert_eq!(err.kind(), ErrorKind::Server);
        assert_eq!(err.message(), "boom");
        assert_eq!(err.upstream_code(), Some("ERR_CODE"));
        assert_eq!(err.upstream_message(), Some("upstream msg"));
        assert_eq!(err.retry_after_seconds(), Some(60));
        assert!(err.is_retryable_transport());
    }

    #[test]
    fn error_accessors_none_when_not_set() {
        let err = Error::new(ErrorKind::Validation, "bad input");
        assert!(err.upstream_code().is_none());
        assert!(err.upstream_message().is_none());
        assert!(err.retry_after_seconds().is_none());
        assert!(!err.is_retryable_transport());
    }
}
