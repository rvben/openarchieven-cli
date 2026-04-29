//! Stream type detection used to pick default output format and progress mode.

use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

pub fn is_tty(stream: Stream) -> bool {
    match stream {
        Stream::Stdout => std::io::stdout().is_terminal(),
        Stream::Stderr => std::io::stderr().is_terminal(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Table,
    Markdown,
}

impl Format {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Some(Format::Json),
            "table" => Some(Format::Table),
            "markdown" => Some(Format::Markdown),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Table => "table",
            Format::Markdown => "markdown",
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve the effective output format.
///
/// Precedence: explicit flag > env var > TTY auto-detect.
pub fn resolve_format(explicit: Option<Format>, env: Option<&str>, stdout_is_tty: bool) -> Format {
    if let Some(f) = explicit {
        return f;
    }
    if let Some(name) = env
        && let Some(f) = Format::parse(name)
    {
        return f;
    }
    if stdout_is_tty {
        Format::Table
    } else {
        Format::Json
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_wins() {
        assert_eq!(
            resolve_format(Some(Format::Markdown), Some("table"), true),
            Format::Markdown
        );
    }

    #[test]
    fn env_overrides_default() {
        assert_eq!(
            resolve_format(None, Some("markdown"), true),
            Format::Markdown
        );
        assert_eq!(resolve_format(None, Some("json"), true), Format::Json);
        assert_eq!(resolve_format(None, Some("table"), false), Format::Table);
    }

    #[test]
    fn env_is_case_insensitive() {
        assert_eq!(resolve_format(None, Some("JSON"), true), Format::Json);
        assert_eq!(resolve_format(None, Some("Table"), false), Format::Table);
        assert_eq!(
            resolve_format(None, Some("MarkDown"), true),
            Format::Markdown
        );
    }

    #[test]
    fn invalid_env_is_ignored() {
        assert_eq!(resolve_format(None, Some("xml"), true), Format::Table);
        assert_eq!(resolve_format(None, Some("xml"), false), Format::Json);
    }

    #[test]
    fn tty_default_is_table() {
        assert_eq!(resolve_format(None, None, true), Format::Table);
    }

    #[test]
    fn pipe_default_is_json() {
        assert_eq!(resolve_format(None, None, false), Format::Json);
    }
}
