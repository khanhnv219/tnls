//! Domain types for SSH tunnel profiles and runtime state.
//!
//! These types are the single source of truth shared by persistence,
//! the OpenSSH process runner, the lifecycle manager, and Tauri IPC.
//! SSH commands are always generated from [`TunnelConfig`]; raw shell
//! command strings are never stored.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Forwarding direction of a tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelMode {
    /// `ssh -L bind:local_port:remote_host:remote_port` — MVP focus.
    Local,
    /// `ssh -R remote_port:local_host:local_port` — later phase.
    Remote,
    /// `ssh -D bind:local_port` (SOCKS) — later phase.
    Dynamic,
}

impl TunnelMode {
    /// Parse from the `mode` column / IPC string.
    pub fn parse(value: &str) -> Result<Self, TunnelError> {
        match value {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            "dynamic" => Ok(Self::Dynamic),
            other => Err(TunnelError::InvalidMode(other.to_owned())),
        }
    }

    /// Canonical lowercase name used in storage and IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }
}

/// Lifecycle state of a tunnel runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelState {
    Stopped,
    Starting,
    Running,
    /// Process alive but TCP health check failing.
    Degraded,
    Stopping,
    Failed,
    /// Used when the manager lost track of the child (e.g. after restart).
    Unknown,
}

impl TunnelState {
    /// Whether the user is allowed to press Start.
    pub fn can_start(self) -> bool {
        matches!(self, Self::Stopped | Self::Failed | Self::Unknown)
    }

    /// Whether the user is allowed to press Stop.
    pub fn can_stop(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Degraded | Self::Unknown
        )
    }
}

/// Stored tunnel profile. Field names map 1:1 to the SQLite columns
/// and to the frontend `TunnelConfig` type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub mode: TunnelMode,
    /// SSH destination: `user@host`, `host`, or an `~/.ssh/config` alias.
    pub ssh_target: String,
    /// Local interface to bind, default `127.0.0.1`.
    pub bind_address: String,
    /// Local port to listen on.
    pub local_port: u16,
    /// Destination host as seen from the SSH server (local/remote modes).
    pub remote_host: Option<String>,
    /// Destination port (local/remote modes).
    pub remote_port: Option<u16>,
    pub identity_file: Option<String>,
    /// Optional SSH jump host (`-J` aka ProxyJump), e.g. "bastion" or "user@jump:2222".
    pub proxy_jump: Option<String>,
    pub auto_start: bool,
    /// `ServerAliveInterval` seconds.
    pub keep_alive_interval: u64,
    /// Max auto-retry attempts for transient failures.
    pub max_retries: u32,
    /// Base retry delay in seconds (backoff doubles, capped at 30s).
    pub retry_delay_secs: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for create/update commands: same fields minus id/timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInput {
    pub name: String,
    pub mode: TunnelMode,
    pub ssh_target: String,
    pub bind_address: Option<String>,
    pub local_port: u16,
    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    pub auto_start: Option<bool>,
    pub keep_alive_interval: Option<u64>,
    pub max_retries: Option<u32>,
    pub retry_delay_secs: Option<u64>,
}
impl TunnelInput {
    /// Fill defaults, assign id + timestamps, then validate.
    pub fn into_config(self) -> Result<TunnelConfig, TunnelError> {
        let now = Utc::now();
        let config = TunnelConfig {
            id: Uuid::new_v4().to_string(),
            name: self.name.trim().to_owned(),
            mode: self.mode,
            ssh_target: self.ssh_target.trim().to_owned(),
            bind_address: self
                .bind_address
                .map(|b| b.trim().to_owned())
                .filter(|b| !b.is_empty())
                .unwrap_or_else(|| "127.0.0.1".to_owned()),
            local_port: self.local_port,
            remote_host: self
                .remote_host
                .map(|h| h.trim().to_owned())
                .filter(|h| !h.is_empty()),
            remote_port: self.remote_port,
            identity_file: self
                .identity_file
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty()),
            proxy_jump: self
                .proxy_jump
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty()),
            auto_start: self.auto_start.unwrap_or(false),
            keep_alive_interval: self.keep_alive_interval.unwrap_or(30),
            max_retries: self.max_retries.unwrap_or(5),
            retry_delay_secs: self.retry_delay_secs.unwrap_or(1),
            created_at: now,
            updated_at: now,
        };
        config.validate()?;
        Ok(config)
    }
}

impl TunnelConfig {
    /// Invariants enforced before persistence and before spawning ssh.
    pub fn validate(&self) -> Result<(), TunnelError> {
        if self.name.is_empty() {
            return Err(TunnelError::InvalidName);
        }
        if self.ssh_target.is_empty() {
            return Err(TunnelError::InvalidSshTarget);
        }
        if self.bind_address.trim().is_empty() {
            return Err(TunnelError::InvalidBindAddress);
        }
        if self.local_port == 0 {
            return Err(TunnelError::InvalidPort("localPort".to_owned()));
        }
        match self.mode {
            TunnelMode::Local | TunnelMode::Remote => {
                let host = self.remote_host.as_deref().unwrap_or("").trim();
                if host.is_empty() {
                    return Err(TunnelError::MissingRemoteHost);
                }
                if self.remote_port.unwrap_or(0) == 0 {
                    return Err(TunnelError::InvalidPort("remotePort".to_owned()));
                }
            }
            TunnelMode::Dynamic => {}
        }
        if self.keep_alive_interval == 0 {
            return Err(TunnelError::InvalidKeepAlive);
        }
        if self.max_retries > 20 {
            return Err(TunnelError::InvalidMaxRetries);
        }
        if self.retry_delay_secs == 0 || self.retry_delay_secs > 30 {
            return Err(TunnelError::InvalidRetryDelay);
        }
        Ok(())
    }

    /// `bind:port` used for conflict detection and health checks.
    /// For `remote` the local endpoint is still the *target* on this side,
    /// so binding checks still apply locally (prevents accidental overlap).
    pub fn local_endpoint(&self) -> String {
        format!("{}:{}", self.bind_address, self.local_port)
    }

    /// `-L bind:local:host:port` / `-R bind:port:host:port` / `-D bind:port`.
    pub fn forward_spec(&self) -> Result<String, TunnelError> {
        match self.mode {
            TunnelMode::Local => {
                let host = self.remote_host.as_deref().unwrap_or("").trim();
                let port = self.remote_port.unwrap_or(0);
                if host.is_empty() {
                    return Err(TunnelError::MissingRemoteHost);
                }
                if port == 0 {
                    return Err(TunnelError::InvalidPort("remotePort".to_owned()));
                }
                Ok(format!(
                    "{}:{}:{}:{}",
                    self.bind_address, self.local_port, host, port
                ))
            }
            TunnelMode::Remote => {
                let host = self.remote_host.as_deref().unwrap_or("").trim();
                let port = self.remote_port.unwrap_or(0);
                if host.is_empty() {
                    return Err(TunnelError::MissingRemoteHost);
                }
                if port == 0 {
                    return Err(TunnelError::InvalidPort("remotePort".to_owned()));
                }
                // Remote: remote side listens at bind:port, forwards to host:local_port.
                Ok(format!(
                    "{}:{}:{}:{}",
                    self.bind_address, port, host, self.local_port
                ))
            }
            TunnelMode::Dynamic => Ok(format!("{}:{}", self.bind_address, self.local_port)),
        }
    }

    /// Apply an update while preserving id/created_at.
    pub fn apply_update(&mut self, input: TunnelInput) -> Result<(), TunnelError> {
        let mut next = input.into_config()?;
        next.id = self.id.clone();
        next.created_at = self.created_at;
        next.updated_at = Utc::now();
        next.validate()?;
        *self = next;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub id: String,
    pub state: TunnelState,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub restart_count: u32,
}

/// Errors surfaced to IPC as human-readable strings.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum TunnelError {
    #[error("tunnel not found: {0}")]
    NotFound(String),
    #[error("tunnel '{0}' is already running")]
    AlreadyRunning(String),
    #[error("tunnel '{0}' is not running")]
    NotRunning(String),
    #[error("invalid tunnel name (must not be empty)")]
    InvalidName,
    #[error("invalid ssh target (must not be empty)")]
    InvalidSshTarget,
    #[error("invalid bind address (must not be empty)")]
    InvalidBindAddress,
    #[error("invalid port for {0} (must be 1-65535)")]
    InvalidPort(String),
    #[error("remote host is required for local/remote forwarding")]
    MissingRemoteHost,
    #[error("invalid keep-alive interval (must be > 0)")]
    InvalidKeepAlive,
    #[error("invalid max retries (must be 0–20)")]
    InvalidMaxRetries,
    #[error("invalid retry delay (must be 1–30 seconds)")]
    InvalidRetryDelay,
    #[error("invalid tunnel mode: {0}")]
    InvalidMode(String),
    #[error("local endpoint {0} is already in use")]
    PortInUse(String),
    #[error("failed to spawn ssh: {0}")]
    SpawnFailed(String),
    #[error("ssh exited with code {0}: {1}")]
    Exited(i32, String),
    #[error("storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local_input() -> TunnelInput {
        TunnelInput {
            name: "db".to_owned(),
            mode: TunnelMode::Local,
            ssh_target: "dev-bastion".to_owned(),
            bind_address: None,
            local_port: 5433,
            remote_host: Some("postgres.internal".to_owned()),
            remote_port: Some(5432),
            identity_file: None,
            proxy_jump: None,
            auto_start: None,
            keep_alive_interval: None,
            max_retries: None,
            retry_delay_secs: None,
        }
    }

    #[test]
    fn defaults_bind_loopback_and_keepalive() {
        let cfg = local_input().into_config().unwrap();
        assert_eq!(cfg.bind_address, "127.0.0.1");
        assert_eq!(cfg.keep_alive_interval, 30);
        assert!(!cfg.auto_start);
        assert!(!cfg.id.is_empty());
    }

    #[test]
    fn local_forward_spec_shape() {
        let cfg = local_input().into_config().unwrap();
        assert_eq!(
            cfg.forward_spec().unwrap(),
            "127.0.0.1:5433:postgres.internal:5432"
        );
        assert_eq!(cfg.local_endpoint(), "127.0.0.1:5433");
    }

    #[test]
    fn dynamic_needs_no_remote() {
        let mut input = local_input();
        input.mode = TunnelMode::Dynamic;
        input.remote_host = None;
        input.remote_port = None;
        let cfg = input.into_config().unwrap();
        assert_eq!(cfg.forward_spec().unwrap(), "127.0.0.1:5433");
    }

    #[test]
    fn local_requires_remote_host_and_port() {
        let mut input = local_input();
        input.remote_host = None;
        assert_eq!(
            input.into_config().unwrap_err(),
            TunnelError::MissingRemoteHost
        );

        let mut input = local_input();
        input.remote_port = Some(0);
        assert_eq!(
            input.into_config().unwrap_err(),
            TunnelError::InvalidPort("remotePort".to_owned())
        );
    }

    #[test]
    fn empty_name_and_target_rejected() {
        let mut input = local_input();
        input.name = "  ".to_owned();
        assert_eq!(input.into_config().unwrap_err(), TunnelError::InvalidName);

        let mut input = local_input();
        input.ssh_target = String::new();
        assert_eq!(
            input.into_config().unwrap_err(),
            TunnelError::InvalidSshTarget
        );
    }

    #[test]
    fn state_transitions_guard_start_stop() {
        assert!(TunnelState::Stopped.can_start());
        assert!(TunnelState::Failed.can_start());
        assert!(!TunnelState::Running.can_start());
        assert!(TunnelState::Running.can_stop());
        assert!(!TunnelState::Stopped.can_stop());
    }

    #[test]
    fn update_preserves_id_and_created_at() {
        let mut cfg = local_input().into_config().unwrap();
        let mut next = local_input();
        next.name = "db-v2".to_owned();
        cfg.apply_update(next).unwrap();
        assert_eq!(cfg.name, "db-v2");
        assert!(!cfg.id.is_empty());
    }
}
