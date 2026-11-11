//! Staged, read-only connection diagnostics for one tunnel profile.
//!
//! 1. **SSH auth / reachability**: `ssh -N -T` with the profile's identity,
//!    jump host and BatchMode, no forwarding; passes on "Authenticated to".
//! 2. **Local forward**: can `bind:local_port` be opened (or is it held by
//!    this tunnel and accepting)? Skipped for `-R`, which listens remotely.
//! 3. **Destination**: from the server, can `remote_host:remote_port` be
//!    opened? `ssh -W host:port` passes on "open confirm" and reports the
//!    server's own open-failure reason. Skipped for `-D` (no fixed target);
//!    for `-R` the target is dialed from this machine.
//!
//! Stages run in order and stop at the first failure. Nothing here starts,
//! stops or changes a tunnel; argv comes from [`crate::ssh`] and exit
//! classification from [`crate::ssh::classify_exit`].

use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::logs::LogLevel;
use crate::ssh::{self, SshChild, SshPurpose};
use crate::tunnel::{TunnelConfig, TunnelError, TunnelMode};

/// Upper bound for one ssh probe (connect timeout is 5 s; jump hosts and
/// key exchange add to it).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    SshAuth,
    LocalForward,
    Destination,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Stage::SshAuth => "SSH auth / reachability",
            Stage::LocalForward => "Local forward",
            Stage::Destination => "Destination reachability",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Pass,
    Fail,
    /// Not applicable to this tunnel mode.
    Skipped,
    /// An earlier stage failed.
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReport {
    pub stage: Stage,
    pub outcome: Outcome,
    /// Short reason, e.g. "auth failed: Permission denied (publickey)".
    pub reason: String,
    /// Relevant ssh lines (debug noise removed), for the copyable summary.
    pub detail: Vec<String>,
    pub at: DateTime<Utc>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub tunnel_id: String,
    pub tunnel_name: String,
    pub started_at: DateTime<Utc>,
    pub stages: Vec<StageReport>,
    /// Plain-text report for the clipboard.
    pub summary: String,
}

/// What one ssh probe printed and how it ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeOutput {
    pub lines: Vec<String>,
    /// Exit code when ssh exited on its own.
    pub exit_code: Option<i32>,
    /// The success marker was seen (probe was then stopped by us).
    pub succeeded: bool,
    pub timed_out: bool,
    /// Index in `lines` of the main connection's "Authenticated to" line.
    /// Lines before it may come from a ProxyJump helper ssh, which shares
    /// stderr and prints its own auth and channel messages.
    pub auth_line: Option<usize>,
}

/// Whether `line` is the *main* connection's auth success. Through a proxy
/// OpenSSH prints "Authenticated to HOST (via proxy) using ..."; the jump
/// helper's own line has an `([ip]:port)` instead.
pub fn is_main_auth_line(line: &str, proxied: bool) -> bool {
    line.contains("Authenticated to ") && (!proxied || line.contains("(via proxy)"))
}

/// Outcome + reason for one stage, before timing is attached.
pub type Verdict = (Outcome, String);

fn is_debug(line: &str) -> bool {
    line.trim_start().to_ascii_lowercase().starts_with("debug")
}

/// Lines worth showing: ssh's warnings and errors, not its debug trace.
pub fn relevant_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| !is_debug(l) && !l.trim().is_empty())
        .cloned()
        .collect()
}

fn last_line(lines: &[String]) -> String {
    relevant_lines(lines)
        .last()
        .cloned()
        .unwrap_or_else(|| "no output".to_owned())
}

/// Stage 1 verdict from an auth probe.
pub fn classify_auth(out: &ProbeOutput, timeout: Duration) -> Verdict {
    if out.succeeded {
        let how = out
            .auth_line
            .and_then(|i| out.lines.get(i))
            .and_then(|l| l.split(" using ").nth(1))
            .map(|m| m.trim_matches(|c| c == '"' || c == '.').to_owned());
        return (
            Outcome::Pass,
            match how {
                Some(method) => format!("authenticated using {method}"),
                None => "authenticated".to_owned(),
            },
        );
    }
    if out.timed_out {
        return (
            Outcome::Fail,
            format!(
                "timeout: no SSH session within {}s (server down, firewalled, or slow jump host)",
                timeout.as_secs()
            ),
        );
    }
    let relevant = relevant_lines(&out.lines);
    let reason = match ssh::classify_exit(out.exit_code, &relevant) {
        TunnelError::SpawnFailed(msg) if msg.starts_with("authentication failed") => {
            format!("auth failed: {}", auth_detail(&relevant))
        }
        TunnelError::SpawnFailed(msg) if msg.starts_with("host key mismatch") => {
            format!("host key verification failed: {}", last_line(&relevant))
        }
        TunnelError::SpawnFailed(msg) if msg.starts_with("cannot resolve") => {
            format!(
                "DNS failure: cannot resolve the SSH host ({})",
                last_line(&relevant)
            )
        }
        TunnelError::SpawnFailed(msg) if msg.starts_with("cannot reach") => {
            let l = msg.to_lowercase();
            if l.contains("refused") {
                format!(
                    "connection refused: no SSH server listening ({})",
                    last_line(&relevant)
                )
            } else {
                format!(
                    "timeout: SSH server did not answer ({})",
                    last_line(&relevant)
                )
            }
        }
        _ => format!(
            "ssh exited (code {}): {}",
            out.exit_code.map_or("?".to_owned(), |c| c.to_string()),
            last_line(&relevant)
        ),
    };
    (Outcome::Fail, reason)
}

/// "Permission denied (publickey)" plus a hint when the key file is the cause.
fn auth_detail(lines: &[String]) -> String {
    let denied = lines
        .iter()
        .rev()
        .find(|l| l.to_lowercase().contains("permission denied"))
        .cloned();
    let key_problem = lines.iter().find(|l| {
        let l = l.to_lowercase();
        l.contains("identity file") && (l.contains("not accessible") || l.contains("no such file"))
            || l.contains("load key")
    });
    match (denied, key_problem) {
        (Some(d), Some(k)) => format!("{d}; {k}"),
        (Some(d), None) => d,
        (None, Some(k)) => k.clone(),
        (None, None) => last_line(lines),
    }
}

/// Stage 3 verdict from a `-W host:port` probe.
pub fn classify_destination(
    host: &str,
    port: u16,
    out: &ProbeOutput,
    timeout: Duration,
) -> Verdict {
    let target = format!("{host}:{port}");
    if out.succeeded {
        return (Outcome::Pass, format!("server opened {target}"));
    }
    // Only the main connection's channel counts: an "open failed" before
    // its auth line belongs to the jump hop (caught by stage 1).
    let after_auth = out.auth_line.map_or(&out.lines[..0], |i| &out.lines[i..]);
    if let Some(msg) = after_auth.iter().find_map(|l| {
        l.split_once("open failed: ")
            .map(|(_, m)| m.trim().to_owned())
    }) {
        return (Outcome::Fail, destination_failure(&target, &msg));
    }
    if out.timed_out {
        return (
            Outcome::Fail,
            format!(
                "timeout: server got no answer from {target} within {}s (host down or port filtered)",
                timeout.as_secs()
            ),
        );
    }
    (
        Outcome::Fail,
        format!(
            "ssh exited before the server answered for {target}: {}",
            last_line(&out.lines)
        ),
    )
}

/// Map the server's open-failure text (`reason: errmsg`) to a reason.
pub fn destination_failure(target: &str, msg: &str) -> String {
    let l = msg.to_lowercase();
    if l.contains("administratively prohibited") {
        return format!(
            "forwarding refused by the server for {target} (AllowTcpForwarding / PermitOpen): {msg}"
        );
    }
    if l.contains("connection refused") {
        return format!("connection refused: nothing listens on {target} ({msg})");
    }
    if l.contains("timed out") {
        return format!("timeout: {target} did not answer the server ({msg})");
    }
    const DNS: [&str; 7] = [
        "name or service not known",
        "no address associated",
        "temporary failure in name resolution",
        "nodename nor servname",
        "name does not resolve",
        "non-recoverable failure in name resolution",
        "unknown host",
    ];
    if DNS.iter().any(|d| l.contains(d)) {
        return format!("DNS failure: the server cannot resolve {target} ({msg})");
    }
    if l.contains("no route to host")
        || l.contains("network is unreachable")
        || l.contains("host is unreachable")
    {
        return format!("unreachable: no route from the server to {target} ({msg})");
    }
    format!("server could not open {target}: {msg}")
}

/// Stage 3 for `-R`: the forward target is dialed from this machine.
pub fn classify_local_dial(target: &str, result: &std::io::Result<()>) -> Verdict {
    match result {
        Ok(()) => (
            Outcome::Pass,
            format!("{target} accepts connections on this machine"),
        ),
        Err(e) => {
            let reason = match e.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    format!("connection refused: nothing listens on {target}")
                }
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                    format!("timeout: {target} did not answer")
                }
                _ if e.to_string().to_lowercase().contains("resolve")
                    || e.to_string().to_lowercase().contains("no such host") =>
                {
                    format!("DNS failure: cannot resolve {target} ({e})")
                }
                _ => format!("cannot reach {target}: {e}"),
            };
            (Outcome::Fail, reason)
        }
    }
}

/// Everything diagnostics need from the outside world, so the stage logic
/// is testable without ssh or sockets.
pub trait DiagnoseEnv {
    /// Run one ssh probe to completion (success marker, exit, or timeout).
    fn probe(&mut self, config: &TunnelConfig, purpose: &SshPurpose) -> ProbeOutput;
    /// Stage 2 verdict (port checks know about running tunnels).
    fn local_forward(&mut self, config: &TunnelConfig) -> Verdict;
    /// TCP dial from this machine (stage 3 for `-R`).
    fn dial(&mut self, host: &str, port: u16) -> std::io::Result<()>;
    /// Called once per finished stage, in order.
    fn on_stage(&mut self, report: &StageReport);
}

/// Run the three stages in order, stopping at the first failure.
pub fn diagnose(config: &TunnelConfig, env: &mut dyn DiagnoseEnv) -> Diagnosis {
    let started_at = Utc::now();
    let mut stages: Vec<StageReport> = Vec::new();
    let mut failed = false;

    for stage in [Stage::SshAuth, Stage::LocalForward, Stage::Destination] {
        let t0 = Instant::now();
        let at = Utc::now();
        let (verdict, detail) = if failed {
            (
                (
                    Outcome::NotRun,
                    "not run: an earlier stage failed".to_owned(),
                ),
                Vec::new(),
            )
        } else {
            run_stage(stage, config, env)
        };
        let report = StageReport {
            stage,
            outcome: verdict.0,
            reason: verdict.1,
            detail,
            at,
            elapsed_ms: t0.elapsed().as_millis() as u64,
        };
        failed |= report.outcome == Outcome::Fail;
        env.on_stage(&report);
        stages.push(report);
    }

    let summary = summarize(config, started_at, &stages);
    Diagnosis {
        tunnel_id: config.id.clone(),
        tunnel_name: config.name.clone(),
        started_at,
        stages,
        summary,
    }
}

fn run_stage(
    stage: Stage,
    config: &TunnelConfig,
    env: &mut dyn DiagnoseEnv,
) -> (Verdict, Vec<String>) {
    match stage {
        Stage::SshAuth => {
            let out = env.probe(config, &SshPurpose::AuthProbe);
            (
                classify_auth(&out, PROBE_TIMEOUT),
                relevant_lines(&out.lines),
            )
        }
        Stage::LocalForward => {
            if config.mode == TunnelMode::Remote {
                return (
                    (
                        Outcome::Skipped,
                        "skipped: a remote forward (-R) listens on the SSH server, not here"
                            .to_owned(),
                    ),
                    Vec::new(),
                );
            }
            (env.local_forward(config), Vec::new())
        }
        Stage::Destination => match config.mode {
            TunnelMode::Dynamic => (
                (
                    Outcome::Skipped,
                    "skipped: dynamic SOCKS (-D) has no fixed destination".to_owned(),
                ),
                Vec::new(),
            ),
            TunnelMode::Remote => {
                let host = config.remote_host.clone().unwrap_or_default();
                let target = format!("{host}:{}", config.local_port);
                let result = env.dial(&host, config.local_port);
                (classify_local_dial(&target, &result), Vec::new())
            }
            TunnelMode::Local => {
                let host = config.remote_host.clone().unwrap_or_default();
                let port = config.remote_port.unwrap_or(0);
                let out = env.probe(
                    config,
                    &SshPurpose::DestinationProbe {
                        host: host.clone(),
                        port,
                    },
                );
                (
                    classify_destination(&host, port, &out, PROBE_TIMEOUT),
                    relevant_lines(&out.lines),
                )
            }
        },
    }
}

/// Log level for a finished stage.
pub fn stage_level(outcome: Outcome) -> LogLevel {
    match outcome {
        Outcome::Pass => LogLevel::Info,
        Outcome::Fail => LogLevel::Error,
        Outcome::Skipped | Outcome::NotRun => LogLevel::Debug,
    }
}

fn outcome_word(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "PASS",
        Outcome::Fail => "FAIL",
        Outcome::Skipped => "SKIP",
        Outcome::NotRun => "----",
    }
}

/// One log line per stage, e.g. "diagnose 1/3 SSH auth / reachability: PASS — ...".
pub fn stage_log_message(report: &StageReport) -> String {
    let n = match report.stage {
        Stage::SshAuth => 1,
        Stage::LocalForward => 2,
        Stage::Destination => 3,
    };
    format!(
        "diagnose {n}/3 {}: {} — {}",
        report.stage.label(),
        outcome_word(report.outcome),
        report.reason
    )
}

fn summarize(config: &TunnelConfig, at: DateTime<Utc>, stages: &[StageReport]) -> String {
    let mut out = format!(
        "tnls diagnostics — {} ({} {})\n{}\n",
        config.name,
        config.mode.as_str(),
        config.ssh_target,
        at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    for (i, s) in stages.iter().enumerate() {
        out.push_str(&format!(
            "{}. {} [{}] {}\n",
            i + 1,
            s.stage.label(),
            outcome_word(s.outcome),
            s.reason
        ));
        for line in &s.detail {
            out.push_str(&format!("     {line}\n"));
        }
    }
    out
}

/// Production probe: spawn ssh, read stderr until the success marker, an
/// exit, or `timeout`; always kill and reap before returning.
///
/// `proxied` (from [`ssh::uses_proxy`]) decides which "Authenticated to"
/// line is the main connection's. The auth probe succeeds on that line;
/// the destination probe succeeds on the first "open confirm" after it.
pub fn run_probe(
    config: &TunnelConfig,
    purpose: &SshPurpose,
    timeout: Duration,
    proxied: bool,
) -> ProbeOutput {
    let (mut child, rx) = match SshChild::spawn_probe(config, purpose) {
        Ok(pair) => pair,
        Err(e) => {
            return ProbeOutput {
                lines: vec![e.to_string()],
                ..Default::default()
            }
        }
    };
    let deadline = Instant::now() + timeout;
    let mut out = ProbeOutput::default();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            out.timed_out = true;
            break;
        }
        match rx.recv_timeout(left.min(Duration::from_millis(200))) {
            Ok(line) => {
                if out.auth_line.is_none() && is_main_auth_line(&line, proxied) {
                    out.auth_line = Some(out.lines.len());
                }
                let hit = match purpose {
                    SshPurpose::AuthProbe => out.auth_line.is_some(),
                    SshPurpose::DestinationProbe { .. } => {
                        out.auth_line.is_some() && line.contains(": open confirm")
                    }
                    SshPurpose::Tunnel { .. } => false,
                };
                out.lines.push(line);
                if hit {
                    out.succeeded = true;
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // Pump finished: ssh closed stderr, i.e. it is exiting.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let exited_on_its_own = !out.succeeded && !out.timed_out;
    let _ = child.kill();
    let code = child.wait();
    out.lines.extend(rx.try_iter());
    if exited_on_its_own {
        out.exit_code = code;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel::TunnelInput;

    fn config(mode: TunnelMode) -> TunnelConfig {
        let mut cfg = TunnelInput {
            name: "9r-homelab".into(),
            mode,
            ssh_target: "homelab".into(),
            bind_address: None,
            local_port: 20129,
            remote_host: Some("127.0.0.1".into()),
            remote_port: Some(20128),
            identity_file: Some("~/.ssh/id_ed25519".into()),
            proxy_jump: None,
            auto_start: None,
            keep_alive_interval: None,
            max_retries: None,
            retry_delay_secs: None,
            tags: None,
        }
        .into_config()
        .unwrap();
        cfg.id = "t1".into();
        cfg
    }

    fn lines(ls: &[&str]) -> Vec<String> {
        ls.iter().map(|s| s.to_string()).collect()
    }

    fn authed() -> ProbeOutput {
        ProbeOutput {
            lines: lines(&[
                "debug1: Connecting to homelab [10.0.0.5] port 22.",
                "debug1: Authentication succeeded (publickey).",
                "Authenticated to homelab ([10.0.0.5]:22) using \"publickey\".",
            ]),
            succeeded: true,
            auth_line: Some(2),
            ..Default::default()
        }
    }

    /// Real ssh binary, no network: a `.invalid` host (RFC 6761) never
    /// resolves, so stage 1 must fail as a DNS failure and stop there.
    #[test]
    #[ignore = "spawns the system ssh; run with --ignored"]
    fn real_ssh_unresolvable_host_fails_stage_one_as_dns() {
        let mut cfg = config(TunnelMode::Local);
        cfg.ssh_target = "nobody@tnls-diagnostics-test.invalid".into();
        cfg.identity_file = None;
        let t0 = Instant::now();
        let out = run_probe(&cfg, &SshPurpose::AuthProbe, PROBE_TIMEOUT, false);
        assert!(
            t0.elapsed() < PROBE_TIMEOUT,
            "probe should end on ssh exit, not timeout"
        );
        assert!(!out.succeeded && !out.timed_out, "{out:?}");
        let (outcome, reason) = classify_auth(&out, PROBE_TIMEOUT);
        assert_eq!(outcome, Outcome::Fail);
        assert!(
            reason.starts_with("DNS failure:"),
            "{reason} / {:?}",
            out.lines
        );
    }

    /// Real ssh against a closed local port: refused, not timeout or auth.
    #[test]
    #[ignore = "spawns the system ssh; run with --ignored"]
    fn real_ssh_closed_port_fails_stage_one_as_refused() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut cfg = config(TunnelMode::Local);
        cfg.ssh_target = format!("ssh://nobody@127.0.0.1:{port}");
        cfg.identity_file = None;
        let out = run_probe(&cfg, &SshPurpose::AuthProbe, PROBE_TIMEOUT, false);
        let (outcome, reason) = classify_auth(&out, PROBE_TIMEOUT);
        assert_eq!(outcome, Outcome::Fail);
        assert!(
            reason.starts_with("connection refused:"),
            "{reason} / {:?}",
            out.lines
        );
    }

    #[test]
    fn jump_helper_auth_line_is_not_the_main_connection() {
        let jump = "Authenticated to jump.example.com ([10.0.0.1]:22) using \"publickey\".";
        let main = "Authenticated to db-host (via proxy) using \"publickey\".";
        assert!(is_main_auth_line(jump, false));
        assert!(!is_main_auth_line(jump, true));
        assert!(is_main_auth_line(main, true));
    }

    #[test]
    fn open_failed_from_the_jump_hop_is_ignored_for_the_destination() {
        // Jump helper failed to reach something before the main auth line;
        // after auth the main channel was refused.
        let out = ProbeOutput {
            lines: lines(&[
                "channel 0: open failed: connect failed: Connection timed out",
                "Authenticated to db-host (via proxy) using \"publickey\".",
                "channel 0: open failed: connect failed: Connection refused",
            ]),
            exit_code: Some(255),
            auth_line: Some(1),
            ..Default::default()
        };
        let (_, reason) = classify_destination("127.0.0.1", 20128, &out, PROBE_TIMEOUT);
        assert!(reason.starts_with("connection refused"), "{reason}");
    }

    /// Scripted world: answers per probe purpose, a stage-2 verdict, a dial result.
    struct FakeEnv {
        auth: ProbeOutput,
        dest: ProbeOutput,
        local: Verdict,
        dial: std::io::ErrorKind,
        probes: Vec<SshPurpose>,
        logged: Vec<String>,
    }

    impl FakeEnv {
        fn healthy() -> Self {
            Self {
                auth: authed(),
                dest: ProbeOutput {
                    lines: lines(&[
                        "Authenticated to homelab ([10.0.0.5]:22) using \"publickey\".",
                        "debug2: channel 0: open confirm rwindow 2097152 rmax 32768",
                    ]),
                    succeeded: true,
                    auth_line: Some(0),
                    ..Default::default()
                },
                local: (Outcome::Pass, "127.0.0.1:20129 is free".into()),
                dial: std::io::ErrorKind::Other,
                probes: Vec::new(),
                logged: Vec::new(),
            }
        }
    }

    impl DiagnoseEnv for FakeEnv {
        fn probe(&mut self, _c: &TunnelConfig, purpose: &SshPurpose) -> ProbeOutput {
            self.probes.push(purpose.clone());
            match purpose {
                SshPurpose::AuthProbe => self.auth.clone(),
                _ => self.dest.clone(),
            }
        }
        fn local_forward(&mut self, _c: &TunnelConfig) -> Verdict {
            self.local.clone()
        }
        fn dial(&mut self, _h: &str, _p: u16) -> std::io::Result<()> {
            Err(self.dial.into())
        }
        fn on_stage(&mut self, report: &StageReport) {
            self.logged.push(stage_log_message(report));
        }
    }

    fn outcomes(d: &Diagnosis) -> Vec<Outcome> {
        d.stages.iter().map(|s| s.outcome).collect()
    }

    #[test]
    fn healthy_tunnel_passes_all_three_stages() {
        let mut env = FakeEnv::healthy();
        let d = diagnose(&config(TunnelMode::Local), &mut env);
        assert_eq!(outcomes(&d), [Outcome::Pass, Outcome::Pass, Outcome::Pass]);
        assert_eq!(d.stages[0].reason, "authenticated using publickey");
        assert_eq!(d.stages[2].reason, "server opened 127.0.0.1:20128");
        assert_eq!(env.logged.len(), 3);
        assert!(env.logged[0].starts_with("diagnose 1/3 SSH auth / reachability: PASS"));
        assert!(d.summary.contains("3. Destination reachability [PASS]"));
    }

    #[test]
    fn wrong_key_fails_at_stage_one_with_auth_failed() {
        let mut env = FakeEnv::healthy();
        env.auth = ProbeOutput {
            lines: lines(&[
                "debug1: Offering public key: C:\\Users\\me/.ssh/id_ed25519 ED25519",
                "debug1: Authentications that can continue: publickey",
                "homelab: Permission denied (publickey).",
            ]),
            exit_code: Some(255),
            ..Default::default()
        };
        let d = diagnose(&config(TunnelMode::Local), &mut env);
        assert_eq!(
            outcomes(&d),
            [Outcome::Fail, Outcome::NotRun, Outcome::NotRun]
        );
        assert!(
            d.stages[0].reason.starts_with("auth failed:"),
            "{}",
            d.stages[0].reason
        );
        assert!(d.stages[0].reason.contains("Permission denied (publickey)"));
        // Stopped at the first failure: no destination probe was run.
        assert_eq!(env.probes, [SshPurpose::AuthProbe]);
        // Debug noise stays out of the detail shown to the user.
        assert_eq!(
            d.stages[0].detail,
            ["homelab: Permission denied (publickey)."]
        );
    }

    #[test]
    fn dead_destination_fails_at_stage_three_not_stage_one() {
        let mut env = FakeEnv::healthy();
        env.dest = ProbeOutput {
            lines: lines(&[
                "Authenticated to homelab ([10.0.0.5]:22) using \"publickey\".",
                "channel 0: open failed: connect failed: Connection refused",
                "stdio forwarding failed",
            ]),
            exit_code: Some(255),
            auth_line: Some(0),
            ..Default::default()
        };
        let d = diagnose(&config(TunnelMode::Local), &mut env);
        assert_eq!(outcomes(&d), [Outcome::Pass, Outcome::Pass, Outcome::Fail]);
        assert!(d.stages[2]
            .reason
            .starts_with("connection refused: nothing listens on 127.0.0.1:20128"));
    }

    #[test]
    fn local_port_problem_fails_at_stage_two() {
        let mut env = FakeEnv::healthy();
        env.local = (
            Outcome::Fail,
            "127.0.0.1:20129 is already in use by node.exe (pid 4321)".into(),
        );
        let d = diagnose(&config(TunnelMode::Local), &mut env);
        assert_eq!(
            outcomes(&d),
            [Outcome::Pass, Outcome::Fail, Outcome::NotRun]
        );
    }

    #[test]
    fn destination_failures_are_told_apart() {
        let t = "db.internal:5432";
        let cases = [
            ("connect failed: Connection timed out", "timeout:"),
            ("connect failed: Name or service not known", "DNS failure:"),
            (
                "connect failed: Temporary failure in name resolution",
                "DNS failure:",
            ),
            ("connect failed: No route to host", "unreachable:"),
            (
                "administratively prohibited: open failed",
                "forwarding refused",
            ),
            ("connect failed: Connection refused", "connection refused:"),
        ];
        for (msg, want) in cases {
            assert!(destination_failure(t, msg).starts_with(want), "{msg}");
        }
    }

    #[test]
    fn auth_stage_distinguishes_reachability_failures() {
        let t = PROBE_TIMEOUT;
        let out = |l: &str| ProbeOutput {
            lines: lines(&[l]),
            exit_code: Some(255),
            ..Default::default()
        };
        let cases = [
            (
                "ssh: connect to host homelab port 22: Connection refused",
                "connection refused:",
            ),
            (
                "ssh: connect to host homelab port 22: Connection timed out",
                "timeout:",
            ),
            (
                "ssh: Could not resolve hostname homelab: No such host is known.",
                "DNS failure:",
            ),
            (
                "Host key verification failed.",
                "host key verification failed",
            ),
        ];
        for (line, want) in cases {
            let (outcome, reason) = classify_auth(&out(line), t);
            assert_eq!(outcome, Outcome::Fail);
            assert!(reason.starts_with(want), "{line} -> {reason}");
        }
        let (_, reason) = classify_auth(
            &ProbeOutput {
                timed_out: true,
                ..Default::default()
            },
            t,
        );
        assert!(reason.starts_with("timeout: no SSH session within 15s"));
    }

    #[test]
    fn modes_skip_what_does_not_apply() {
        let mut env = FakeEnv::healthy();
        let d = diagnose(&config(TunnelMode::Dynamic), &mut env);
        assert_eq!(
            outcomes(&d),
            [Outcome::Pass, Outcome::Pass, Outcome::Skipped]
        );

        let mut env = FakeEnv::healthy();
        env.dial = std::io::ErrorKind::ConnectionRefused;
        let d = diagnose(&config(TunnelMode::Remote), &mut env);
        assert_eq!(
            outcomes(&d),
            [Outcome::Pass, Outcome::Skipped, Outcome::Fail]
        );
        assert!(d.stages[2]
            .reason
            .starts_with("connection refused: nothing listens on 127.0.0.1:20129"));
        // -R destination is dialed locally, never via an ssh -W probe.
        assert_eq!(env.probes, [SshPurpose::AuthProbe]);
    }
}
