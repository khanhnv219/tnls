//! Per-tunnel log entries: ssh stderr lines stamped with the time they were
//! collected and a level inferred from OpenSSH's own wording.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    /// When the supervisor collected the line (within one poll interval,
    /// 250 ms, of ssh writing it).
    pub at: DateTime<Utc>,
    pub level: LogLevel,
    pub text: String,
}

impl LogLine {
    /// An ssh stderr line; the level is inferred from its wording.
    pub fn new(at: DateTime<Utc>, text: String) -> Self {
        Self {
            at,
            level: classify_level(&text),
            text,
        }
    }

    /// A line written by tnls itself (lifecycle, health, retries), always
    /// prefixed `tnls:` so it is distinguishable from ssh output.
    pub fn tnls(at: DateTime<Utc>, level: LogLevel, message: &str) -> Self {
        Self {
            at,
            level,
            text: format!("tnls: {message}"),
        }
    }
}

const ERROR_PREFIXES: [&str; 3] = ["ssh:", "fatal:", "error:"];
const ERROR_WORDS: [&str; 15] = [
    "error",
    "already in use",
    "failed",
    "failure",
    "denied",
    "refused",
    "timed out",
    "unreachable",
    "no route",
    "could not",
    "cannot",
    "broken pipe",
    "closed by remote",
    "host key verification",
    "identification has changed",
];

/// Best-effort level for one ssh stderr line. `debug1:`..`debug3:` prefixes
/// win over any error words they contain, because verbose ssh reports
/// recoverable attempts ("... failed, trying next key") at debug level.
pub fn classify_level(line: &str) -> LogLevel {
    let l = line.trim().to_lowercase();
    if l.starts_with("debug1:") || l.starts_with("debug2:") || l.starts_with("debug3:") {
        return LogLevel::Debug;
    }
    if l.starts_with("warning:") || l.contains("warning") {
        return LogLevel::Warn;
    }
    if ERROR_PREFIXES.iter().any(|p| l.starts_with(p)) || ERROR_WORDS.iter().any(|w| l.contains(w))
    {
        return LogLevel::Error;
    }
    LogLevel::Info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_openssh_lines() {
        let cases = [
            ("debug1: Connecting to bastion port 22.", LogLevel::Debug),
            (
                "debug1: Authentications that can continue: publickey; failed",
                LogLevel::Debug,
            ),
            (
                "Warning: Permanently added 'h' (ED25519) to the list of known hosts.",
                LogLevel::Warn,
            ),
            (
                "ssh: connect to host h port 22: Connection timed out",
                LogLevel::Error,
            ),
            (
                "channel 3: open failed: connect failed: Connection refused",
                LogLevel::Error,
            ),
            ("Permission denied (publickey).", LogLevel::Error),
            (
                "bind [127.0.0.1]:5433: Address already in use",
                LogLevel::Error,
            ),
            ("Host key verification failed.", LogLevel::Error),
            ("OpenSSH_for_Windows_9.5p1, LibreSSL 3.8.2", LogLevel::Info),
        ];
        for (line, want) in cases {
            assert_eq!(classify_level(line), want, "{line}");
        }
    }

    #[test]
    fn serializes_level_uppercase_and_fields_camel_case() {
        let line = LogLine::new(
            "2026-11-09T10:00:00Z".parse().unwrap(),
            "Warning: something".into(),
        );
        let json = serde_json::to_value(&line).unwrap();
        assert_eq!(json["level"], "WARN");
        assert_eq!(json["text"], "Warning: something");
        assert_eq!(json["at"], "2026-11-09T10:00:00Z");
    }
}
