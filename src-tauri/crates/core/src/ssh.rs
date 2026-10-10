//! OpenSSH process runner: build argv from [`TunnelConfig`], spawn `ssh`
//! without a shell, stream stderr, classify early failures.
//!
//! Design rules (see docs/DESIGN.md):
//! - argv array only, never a shell string (no injection, no quoting bugs);
//! - always `-N -T -o ExitOnForwardFailure=yes -o BatchMode=yes`;
//! - keepalive from profile (`ServerAliveInterval/CountMax`);
//! - `ssh` is resolved from PATH; override via `TNLS_SSH_BINARY` (tests).

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use crate::tunnel::{TunnelConfig, TunnelError};

/// Default keepalive probes before the server drops a dead connection.
const DEFAULT_ALIVE_COUNT_MAX: u64 = 3;
/// How long to wait for `ssh` stderr before assuming startup is in flight.
pub const STARTUP_GRACE_MS: u64 = 400;

/// Resolve the ssh binary: env override first, then PATH lookup.
pub fn ssh_binary() -> OsString {
    if let Ok(custom) = std::env::var("TNLS_SSH_BINARY") {
        if !custom.trim().is_empty() {
            return OsString::from(custom);
        }
    }
    #[cfg(windows)]
    const CANDIDATES: [&str; 2] = ["ssh.exe", "ssh"];
    #[cfg(not(windows))]
    const CANDIDATES: [&str; 1] = ["ssh"];
    for name in CANDIDATES {
        if let Some(path) = find_on_path(name) {
            return path.into_os_string();
        }
    }
    OsString::from(CANDIDATES[0])
}

fn find_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Probe that `ssh` exists and reports a version on stderr/stdout.
pub fn probe_ssh() -> Result<String, TunnelError> {
    let out = Command::new(ssh_binary())
        .arg("-V")
        .output()
        .map_err(|e| TunnelError::SpawnFailed(format!("ssh -V: {e}")))?;
    let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    let line = text.lines().next().unwrap_or("").trim().to_owned();
    if line.is_empty() {
        return Err(TunnelError::SpawnFailed("ssh -V produced no output".into()));
    }
    Ok(line)
}

/// Build the full ssh argv (program + args) from a validated config.
/// Never joins into a shell string: caller passes the vec to [`Command`].
pub fn build_argv(config: &TunnelConfig) -> Result<Vec<OsString>, TunnelError> {
    config.validate()?;
    let forward = config.forward_spec()?;
    let flag = match config.mode {
        crate::tunnel::TunnelMode::Local => "-L",
        crate::tunnel::TunnelMode::Remote => "-R",
        crate::tunnel::TunnelMode::Dynamic => "-D",
    };
    let mut argv: Vec<OsString> = vec![
        ssh_binary(),
        "-N".into(),
        "-T".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        format!("ServerAliveInterval={}", config.keep_alive_interval).into(),
        "-o".into(),
        format!("ServerAliveCountMax={DEFAULT_ALIVE_COUNT_MAX}").into(),
    ];
    if let Some(jump) = config
        .proxy_jump
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        argv.push("-J".into());
        argv.push(jump.trim().to_owned().into());
    }
    argv.push(flag.into());
    argv.push(forward.into());
    if let Some(key) = config.identity_file.as_deref() {
        argv.push("-i".into());
        argv.push(key.into());
    }
    argv.push(config.ssh_target.clone().into());
    Ok(argv)
}

/// A spawned ssh child with a background stderr pump.
pub struct SshChild {
    child: Child,
    stderr_handle: Option<JoinHandle<Vec<String>>>,
}

impl SshChild {
    /// Spawn `ssh` for the config. Returns the child plus a channel that
    /// receives stderr lines as they arrive (for logs + failure sniffing).
    pub fn spawn(config: &TunnelConfig) -> Result<(Self, Receiver<String>), TunnelError> {
        let argv = build_argv(config)?;
        let (program, args) = argv.split_first().expect("argv always has program");
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            cmd.creation_flags(0x08000000 /* CREATE_NO_WINDOW */);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| TunnelError::SpawnFailed(format!("spawn ssh: {e}")))?;
        let stderr: ChildStderr = child
            .stderr
            .take()
            .ok_or_else(|| TunnelError::SpawnFailed("ssh stderr was not captured".to_owned()))?;
        let (tx, rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name(format!("ssh-stderr-{}", config.id))
            .spawn(move || {
                let reader = BufReader::new(stderr);
                let mut kept = Vec::new();
                for line in reader.lines().map_while(Result::ok) {
                    kept.push(line.clone());
                    let _ = tx.send(line);
                }
                kept
            })
            .map_err(|e| TunnelError::SpawnFailed(format!("spawn stderr pump: {e}")))?;
        Ok((
            Self {
                child,
                stderr_handle: Some(handle),
            },
            rx,
        ))
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Non-blocking exit check.
    pub fn try_exit(&mut self) -> Result<Option<i32>, TunnelError> {
        self.child
            .try_wait()
            .map(|status| status.and_then(|s| s.code()))
            .map_err(|e| TunnelError::Storage(format!("poll ssh exit: {e}")))
    }

    /// SIGKILL-equivalent; caller should `wait()` after.
    pub fn kill(&mut self) -> Result<(), TunnelError> {
        self.child
            .kill()
            .map_err(|e| TunnelError::Storage(format!("kill ssh: {e}")))
    }

    /// Wait for exit and collect tail of stderr (bounded).
    pub fn wait_with_tail(mut self, tail: usize) -> (Option<i32>, Vec<String>) {
        let code = self.child.wait().ok().and_then(|s| s.code());
        let lines = self
            .stderr_handle
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let start = lines.len().saturating_sub(tail);
        (code, lines[start..].to_vec())
    }
}

/// Classify an early ssh exit into a user-facing error.
pub fn classify_exit(code: Option<i32>, stderr_tail: &[String]) -> TunnelError {
    let text = stderr_tail.join("\n").to_lowercase();
    let tail = stderr_tail.join("\n");
    if text.contains("address already in use")
        || text.contains("cannot listen to port")
        || text.contains("bind: address already in use")
    {
        let endpoint = stderr_tail
            .iter()
            .find(|l| l.contains(':'))
            .cloned()
            .unwrap_or_else(|| "local port".to_owned());
        return TunnelError::PortInUse(endpoint);
    }
    if text.contains("permission denied")
        || text.contains("authentication failed")
        || text.contains("no such identity")
    {
        return TunnelError::SpawnFailed(format!("authentication failed: {tail}"));
    }
    if text.contains("host key verification failed")
        || text.contains("remote host identification has changed")
    {
        return TunnelError::SpawnFailed(format!("host key mismatch: {tail}"));
    }
    if text.contains("administratively prohibited") || text.contains("open failed") {
        return TunnelError::SpawnFailed(format!("server refused forwarding: {tail}"));
    }
    if text.contains("could not resolve hostname") || text.contains("name or service not known") {
        return TunnelError::SpawnFailed(format!("cannot resolve SSH host: {tail}"));
    }
    if text.contains("connection refused") || text.contains("connection timed out") {
        return TunnelError::SpawnFailed(format!("cannot reach SSH server: {tail}"));
    }
    TunnelError::Exited(code.unwrap_or(-1), tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel::{TunnelInput, TunnelMode};

    fn local_config() -> TunnelConfig {
        TunnelInput {
            name: "db".to_owned(),
            mode: TunnelMode::Local,
            ssh_target: "dev-bastion".to_owned(),
            bind_address: None,
            local_port: 5433,
            remote_host: Some("postgres.internal".to_owned()),
            remote_port: Some(5432),
            identity_file: Some("~/.ssh/id_ed25519".to_owned()),
            proxy_jump: None,
            auto_start: None,
            keep_alive_interval: None,
            max_retries: None,
            retry_delay_secs: None,
        }
        .into_config()
        .unwrap()
    }

    fn argv_strings(config: &TunnelConfig) -> Vec<String> {
        build_argv(config)
            .unwrap()
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn argv_has_safe_defaults_no_shell() {
        let args = argv_strings(&local_config());
        // argv[0] is the resolved binary; the rest must match exactly.
        assert_eq!(
            &args[1..],
            &[
                "-N",
                "-T",
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "BatchMode=yes",
                "-o",
                "ServerAliveInterval=30",
                "-o",
                "ServerAliveCountMax=3",
                "-L",
                "127.0.0.1:5433:postgres.internal:5432",
                "-i",
                "~/.ssh/id_ed25519",
                "dev-bastion",
            ]
        );
        // No shell metacharacters smuggled into any single arg.
        for arg in &args[1..] {
            assert!(!arg.contains("&&") && !arg.contains(';') && !arg.contains('|'));
        }
    }

    #[test]
    fn dynamic_uses_d_flag_without_remote() {
        let mut config = local_config();
        config.mode = TunnelMode::Dynamic;
        config.remote_host = None;
        config.remote_port = None;
        let args = argv_strings(&config);
        assert!(args.contains(&"-D".to_owned()));
        assert!(args.contains(&"127.0.0.1:5433".to_owned()));
        assert!(!args.iter().any(|a| a == "postgres.internal:5432"));
    }

    #[test]
    fn custom_keepalive_flows_into_argv() {
        let mut config = local_config();
        config.keep_alive_interval = 15;
        let args = argv_strings(&config);
        assert!(args.contains(&"ServerAliveInterval=15".to_owned()));
    }

    #[test]
    fn invalid_config_never_builds_argv() {
        let mut config = local_config();
        config.local_port = 0;
        assert!(build_argv(&config).is_err());
    }

    #[test]
    fn proxy_jump_adds_j_flag() {
        let mut config = local_config();
        config.proxy_jump = Some("jump-host".into());
        let args = argv_strings(&config);
        let idx = args.iter().position(|a| a == "-J").expect("-J present");
        assert_eq!(args[idx + 1], "jump-host");
    }

    #[test]
    fn classify_port_conflict() {
        let err = classify_exit(
            Some(255),
            &[
                "channel_setup_fwd_listener_tcpip: cannot listen to port: 5433".to_owned(),
                "Could not request local forwarding.".to_owned(),
            ],
        );
        assert!(matches!(err, TunnelError::PortInUse(_)));
    }

    #[test]
    fn classify_auth_and_hostkey() {
        let auth = classify_exit(Some(255), &["Permission denied (publickey).".to_owned()]);
        assert!(matches!(auth, TunnelError::SpawnFailed(_)));
        let hostkey = classify_exit(Some(255), &["Host key verification failed.".to_owned()]);
        assert!(matches!(hostkey, TunnelError::SpawnFailed(_)));
    }

    #[test]
    fn classify_generic_exit_keeps_tail() {
        let err = classify_exit(Some(1), &["boom".to_owned()]);
        assert_eq!(err, TunnelError::Exited(1, "boom".to_owned()));
    }

    #[test]
    fn probe_finds_real_ssh() {
        // Ground truth: this environment ships OpenSSH.
        let version = probe_ssh().expect("ssh must be installed");
        assert!(version.to_lowercase().contains("openssh"), "got: {version}");
    }
}
