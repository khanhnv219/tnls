//! Tunnel lifecycle manager: one supervisor owning every tunnel runtime.
//!
//! The manager is transport-agnostic: it drives [`ManagedChild`]s produced
//! by a [`Spawner`]. Production uses [`SshSpawner`] (real OpenSSH); tests
//! inject a fake. All timing flows through `poll(now)`, so behavior is
//! deterministic under test.
//!
//! Retry policy (see docs/DESIGN.md): exponential-ish backoff, bounded
//! retries, counter reset after 60 s of stable uptime, retry ONLY for
//! transient failures (unreachable server / unknown remote exits). Config
//! errors (port conflict, auth, host key) fail immediately.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::health::{self, HEALTH_TIMEOUT};
use crate::ssh::{self, SshChild, STARTUP_GRACE_MS};
use crate::tunnel::{TunnelConfig, TunnelError, TunnelState, TunnelStatus};

/// Backoff between retries: 1s, 2s, 5s, 10s, then 30s cap.
const DEFAULT_RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];
/// Retries allowed in a row before giving up.
const DEFAULT_MAX_RETRIES: u32 = 5;
/// Uptime after which the retry counter resets.
const STABLE_UPTIME: Duration = Duration::from_secs(60);
/// stderr lines kept per tunnel (ring buffer).
const LOG_CAP: usize = 500;
/// stderr tail length used for failure classification.
const FAILURE_TAIL: usize = 50;

/// Minimal child-process surface the manager needs. Implemented by
/// [`RealChild`] (OpenSSH) and by test fakes.
pub trait ManagedChild: Send {
    fn pid(&self) -> u32;
    /// `Ok(true)` while the process is still alive.
    fn is_alive(&mut self) -> Result<bool, TunnelError>;
    fn kill(&mut self) -> Result<(), TunnelError>;
    /// Blocking reap: wait for exit, return code + stderr tail.
    fn reap(&mut self, tail: usize) -> (Option<i32>, Vec<String>);
    /// Non-blocking drain of stderr lines received so far.
    fn drain_stderr(&mut self) -> Vec<String>;
}
/// Decides whether a local endpoint currently accepts TCP connections.
/// Production dials for real; tests inject scripted answers.
pub trait HealthChecker: Send {
    fn check(&mut self, endpoint: &str) -> bool;
}

/// Production checker: real TCP dial with a short timeout.
pub struct TcpHealthChecker;

impl HealthChecker for TcpHealthChecker {
    fn check(&mut self, endpoint: &str) -> bool {
        health::tcp_check(endpoint, HEALTH_TIMEOUT)
    }
}

/// Something that can launch a tunnel child process.
pub trait Spawner: Send {
    fn spawn(&mut self, config: &TunnelConfig) -> Result<Box<dyn ManagedChild>, TunnelError>;
}

/// Production spawner: real `ssh` via [`SshChild`].
pub struct SshSpawner;

impl Spawner for SshSpawner {
    fn spawn(&mut self, config: &TunnelConfig) -> Result<Box<dyn ManagedChild>, TunnelError> {
        let (child, rx) = SshChild::spawn(config)?;
        Ok(Box::new(RealChild {
            child: Some(child),
            rx,
        }))
    }
}

struct RealChild {
    child: Option<SshChild>,
    rx: Receiver<String>,
}

impl ManagedChild for RealChild {
    fn pid(&self) -> u32 {
        self.child.as_ref().map(SshChild::pid).unwrap_or(0)
    }

    fn is_alive(&mut self) -> Result<bool, TunnelError> {
        match self.child.as_mut() {
            Some(c) => Ok(c.try_exit()?.is_none()),
            None => Ok(false),
        }
    }

    fn kill(&mut self) -> Result<(), TunnelError> {
        match self.child.as_mut() {
            Some(c) => c.kill(),
            None => Ok(()),
        }
    }

    fn reap(&mut self, tail: usize) -> (Option<i32>, Vec<String>) {
        match self.child.take() {
            Some(c) => c.wait_with_tail(tail),
            None => (None, Vec::new()),
        }
    }

    fn drain_stderr(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(line) = self.rx.try_recv() {
            out.push(line);
        }
        out
    }
}

/// Manager-level event for Tauri to forward to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerEvent {
    pub id: String,
    pub kind: ManagerEventKind,
    pub at: DateTime<Utc>,
}

/// What happened to a tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ManagerEventKind {
    Started { pid: u32 },
    StateChanged(TunnelState),
    RetryScheduled { attempt: u32, delay_secs: u64 },
    Failed { error: String },
    Stopped,
    HealthChanged { healthy: bool },
}

struct Runtime {
    status: TunnelStatus,
    child: Option<Box<dyn ManagedChild>>,
    logs: VecDeque<String>,
    /// stderr of the current incarnation, for failure classification.
    incarnation_lines: Vec<String>,
    incarnation_started: Option<Instant>,
    stop_requested: bool,
    next_retry_at: Option<Instant>,
    /// Last TCP health result; None = not yet checked this incarnation.
    last_healthy: Option<bool>,
    last_health_check: Option<Instant>,
}

impl Runtime {
    fn fresh(id: &str) -> Self {
        Self {
            status: TunnelStatus {
                id: id.to_owned(),
                state: TunnelState::Stopped,
                pid: None,
                started_at: None,
                last_error: None,
                restart_count: 0,
            },
            child: None,
            logs: VecDeque::new(),
            incarnation_lines: Vec::new(),
            incarnation_started: None,
            stop_requested: false,
            next_retry_at: None,
            last_healthy: None,
            last_health_check: None,
        }
    }

    fn push_logs(&mut self, lines: Vec<String>) {
        for line in lines {
            if self.logs.len() >= LOG_CAP {
                self.logs.pop_front();
            }
            self.logs.push_back(line);
        }
    }
}

/// Supervisor for all tunnel processes.
pub struct TunnelManager<S: Spawner = SshSpawner> {
    spawner: S,
    checker: Box<dyn HealthChecker>,
    configs: HashMap<String, TunnelConfig>,
    runtimes: HashMap<String, Runtime>,
    events: VecDeque<ManagerEvent>,
    retry_delays: Vec<Duration>,
    max_retries: u32,
    startup_grace: Duration,
    health_interval: Duration,
}

impl<S: Spawner> TunnelManager<S> {
    pub fn new(spawner: S) -> Self {
        Self {
            spawner,
            checker: Box::new(TcpHealthChecker),
            configs: HashMap::new(),
            runtimes: HashMap::new(),
            events: VecDeque::new(),
            retry_delays: DEFAULT_RETRY_DELAYS.to_vec(),
            max_retries: DEFAULT_MAX_RETRIES,
            startup_grace: Duration::from_millis(STARTUP_GRACE_MS),
            health_interval: Duration::from_secs(10),
        }
    }

    /// Override the TCP dial (tests inject scripted answers).
    pub fn set_health_checker(&mut self, checker: Box<dyn HealthChecker>) {
        self.checker = checker;
    }

    /// How often a Running tunnel is re-dialed.
    pub fn set_health_interval(&mut self, interval: Duration) {
        self.health_interval = interval;
    }

    /// Override retry schedule (tests use `[ZERO]` for instant retries).
    pub fn set_retry_delays(&mut self, delays: Vec<Duration>) {
        self.retry_delays = delays;
    }

    pub fn set_max_retries(&mut self, max: u32) {
        self.max_retries = max;
    }

    pub fn set_startup_grace(&mut self, grace: Duration) {
        self.startup_grace = grace;
    }

    fn emit(&mut self, id: &str, kind: ManagerEventKind) {
        self.events.push_back(ManagerEvent {
            id: id.to_owned(),
            kind,
            at: Utc::now(),
        });
    }

    fn set_state(&mut self, id: &str, state: TunnelState) {
        if let Some(rt) = self.runtimes.get_mut(id) {
            rt.status.state = state;
        }
        self.emit(id, ManagerEventKind::StateChanged(state));
    }

    /// Re-dial a Running/Degraded tunnel at most every `health_interval`.
    /// On change, transition state + emit both StateChanged and
    /// HealthChanged so the UI updates either way it listens.
    fn maybe_recheck_health(&mut self, id: &str, now: Instant) {
        let due = {
            let rt = &self.runtimes[id];
            rt.last_health_check
                .is_none_or(|t| now - t >= self.health_interval)
        };
        if !due {
            return;
        }
        let endpoint = match self.configs.get(id) {
            Some(c) => c.local_endpoint(),
            None => return,
        };
        let healthy = self.checker.check(&endpoint);
        let changed = {
            let rt = self.runtimes.get_mut(id).expect("runtime exists");
            rt.last_health_check = Some(now);
            let changed = rt.last_healthy != Some(healthy);
            rt.last_healthy = Some(healthy);
            changed
        };
        if !changed {
            return;
        }
        let state = self.runtimes[id].status.state;
        if healthy && state == TunnelState::Degraded {
            self.set_state(id, TunnelState::Running);
            self.emit(id, ManagerEventKind::HealthChanged { healthy: true });
        } else if !healthy && state == TunnelState::Running {
            self.set_state(id, TunnelState::Degraded);
            self.emit(id, ManagerEventKind::HealthChanged { healthy: false });
        }
    }

    pub fn upsert_config(&mut self, config: TunnelConfig) -> bool {
        config
            .validate()
            .expect("manager only stores validated configs");
        let running = self
            .runtimes
            .get(&config.id)
            .is_some_and(|rt| rt.child.is_some());
        self.configs.insert(config.id.clone(), config);
        running
    }

    pub fn config(&self, id: &str) -> Option<&TunnelConfig> {
        self.configs.get(id)
    }

    /// Remove a profile, stopping it first when running.
    pub fn remove_config(&mut self, id: &str) -> Result<(), TunnelError> {
        if self.runtimes.get(id).is_some_and(|rt| rt.child.is_some()) {
            self.stop(id)?;
        }
        self.configs.remove(id);
        self.runtimes.remove(id);
        Ok(())
    }

    /// Start a stopped/failed tunnel.
    pub fn start(&mut self, id: &str) -> Result<TunnelStatus, TunnelError> {
        let config = self
            .configs
            .get(id)
            .ok_or_else(|| TunnelError::NotFound(id.to_owned()))?
            .clone();
        let already = self
            .runtimes
            .get(id)
            .is_some_and(|rt| !rt.status.state.can_start());
        if already {
            return Err(TunnelError::AlreadyRunning(id.to_owned()));
        }
        check_port_free(&config)?;
        let mut child = self.spawner.spawn(&config)?;
        // Reap instant-exit failures (bad config surfaces fast thanks to
        // ExitOnForwardFailure) instead of reporting a phantom Running.
        // Route through the retry policy: transient errors get retries,
        // config errors fail immediately.
        if !child.is_alive()? {
            let (code, tail) = child.reap(FAILURE_TAIL);
            let err = ssh::classify_exit(code, &tail);
            let rt = self
                .runtimes
                .entry(id.to_owned())
                .or_insert_with(|| Runtime::fresh(id));
            rt.push_logs(tail);
            rt.incarnation_started = Some(Instant::now());
            rt.last_healthy = None;
            rt.last_health_check = None;
            self.handle_failure(id, Instant::now(), err.clone());
            return Err(err);
        }
        let pid = child.pid();
        let now_wall = Utc::now();
        let rt = self
            .runtimes
            .entry(id.to_owned())
            .or_insert_with(|| Runtime::fresh(id));
        rt.child = Some(child);
        rt.incarnation_lines.clear();
        rt.incarnation_started = Some(Instant::now());
        rt.last_healthy = None;
        rt.last_health_check = None;
        rt.stop_requested = false;
        rt.next_retry_at = None;
        rt.status.state = TunnelState::Starting;
        rt.status.pid = Some(pid);
        rt.status.started_at = Some(now_wall);
        rt.status.last_error = None;
        rt.status.restart_count = 0;
        self.emit(id, ManagerEventKind::Started { pid });
        self.emit(id, ManagerEventKind::StateChanged(TunnelState::Starting));
        Ok(self.runtimes[id].status.clone())
    }

    /// Stop a running/starting tunnel and reap the child.
    pub fn stop(&mut self, id: &str) -> Result<TunnelStatus, TunnelError> {
        let rt = self
            .runtimes
            .get_mut(id)
            .ok_or_else(|| TunnelError::NotFound(id.to_owned()))?;
        if !rt.status.state.can_stop() {
            return Err(TunnelError::NotRunning(id.to_owned()));
        }
        rt.stop_requested = true;
        rt.next_retry_at = None;
        if let Some(mut child) = rt.child.take() {
            let _ = child.kill();
            let (_, tail) = child.reap(FAILURE_TAIL);
            rt.push_logs(tail);
        }
        rt.status.state = TunnelState::Stopped;
        rt.status.pid = None;
        rt.status.started_at = None;
        rt.status.last_error = None;
        self.emit(id, ManagerEventKind::Stopped);
        self.emit(id, ManagerEventKind::StateChanged(TunnelState::Stopped));
        Ok(self.runtimes[id].status.clone())
    }

    pub fn restart(&mut self, id: &str) -> Result<TunnelStatus, TunnelError> {
        if self
            .runtimes
            .get(id)
            .is_some_and(|rt| rt.status.state.can_stop())
        {
            self.stop(id)?;
        }
        self.start(id)
    }

    /// Stop every running tunnel (graceful shutdown path).
    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self
            .runtimes
            .iter()
            .filter(|(_, rt)| rt.child.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.stop(&id);
        }
    }

    /// Supervisor tick: promote Starting→Running, reap exits, schedule
    /// retries, collect stderr into per-tunnel logs.
    pub fn poll(&mut self, now: Instant) {
        // Due retries first (no child, timer elapsed, not stopped).
        let due: Vec<String> = self
            .runtimes
            .iter()
            .filter(|(_, rt)| {
                rt.child.is_none()
                    && !rt.stop_requested
                    && rt.next_retry_at.is_some_and(|t| now >= t)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            // Port may have been taken since the failure; surface it instead
            // of burning a retry on a doomed spawn.
            let config = match self.configs.get(&id).cloned() {
                Some(c) => c,
                None => continue,
            };
            if let Err(err) = check_port_free(&config) {
                self.fail_now(&id, err);
                continue;
            }
            match self.spawner.spawn(&config) {
                Ok(child) => {
                    let pid = child.pid();
                    let rt = self.runtimes.get_mut(&id).expect("runtime exists");
                    rt.child = Some(child);
                    rt.incarnation_lines.clear();
                    rt.incarnation_started = Some(now);
                    rt.last_healthy = None;
                    rt.last_health_check = None;
                    rt.status.state = TunnelState::Starting;
                    rt.status.pid = Some(pid);
                    rt.status.started_at = Some(Utc::now());
                    self.emit(&id, ManagerEventKind::Started { pid });
                    self.emit(&id, ManagerEventKind::StateChanged(TunnelState::Starting));
                }
                Err(err) => {
                    self.fail_now(&id, err);
                }
            }
        }

        let active: Vec<String> = self
            .runtimes
            .iter()
            .filter(|(_, rt)| rt.child.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        for id in active {
            // Drain stderr into logs (split borrows of spawner/runtimes/events
            // handled by working through local state first).
            let (lines, alive, reap) = {
                let rt = self.runtimes.get_mut(&id).expect("filtered");
                let child = rt.child.as_mut().expect("filtered");
                let lines = child.drain_stderr();
                let alive = child.is_alive();
                let reap = match &alive {
                    Ok(false) => Some(child.reap(FAILURE_TAIL)),
                    _ => None,
                };
                (lines, alive, reap)
            };
            {
                let rt = self.runtimes.get_mut(&id).expect("filtered");
                rt.push_logs(lines.clone());
                rt.incarnation_lines.extend(lines);
            }
            match alive {
                Err(err) => {
                    self.runtimes.get_mut(&id).expect("filtered").child = None;
                    self.fail_now(&id, err);
                }
                Ok(true) => {
                    let past_grace = {
                        let rt = &self.runtimes[&id];
                        rt.incarnation_started
                            .is_some_and(|t| now - t >= self.startup_grace)
                    };
                    let state = self.runtimes[&id].status.state;
                    if state == TunnelState::Starting && past_grace {
                        let config = self.configs.get(&id).expect("has config");
                        // Remote forwarding has no local listening socket to probe;
                        // process-alive is the only meaningful health signal.
                        if config.mode == crate::tunnel::TunnelMode::Remote {
                            self.set_state(&id, TunnelState::Running);
                        } else {
                            let endpoint = config.local_endpoint();
                            let healthy = self.checker.check(&endpoint);
                            self.runtimes.get_mut(&id).expect("filtered").last_healthy =
                                Some(healthy);
                            if healthy {
                                self.set_state(&id, TunnelState::Running);
                            } else {
                                self.set_state(&id, TunnelState::Degraded);
                                self.emit(&id, ManagerEventKind::HealthChanged { healthy: false });
                            }
                        }
                    } else if state == TunnelState::Running || state == TunnelState::Degraded {
                        // Only re-check health for local/dynamic.
                        let is_remote = self
                            .configs
                            .get(&id)
                            .is_some_and(|c| c.mode == crate::tunnel::TunnelMode::Remote);
                        if !is_remote {
                            self.maybe_recheck_health(&id, now);
                        }
                    }
                }
                Ok(false) => {
                    let (code, tail) = reap.expect("reaped on exit");
                    {
                        let rt = self.runtimes.get_mut(&id).expect("filtered");
                        rt.child = None;
                        rt.push_logs(tail.clone());
                        rt.incarnation_lines.extend(tail.clone());
                    }
                    if self.runtimes[&id].stop_requested {
                        self.set_state(&id, TunnelState::Stopped);
                        continue;
                    }
                    let err = {
                        let rt = &self.runtimes[&id];
                        ssh::classify_exit(code, &rt.incarnation_lines)
                    };
                    self.handle_failure(&id, now, err);
                }
            }
        }
    }

    /// Record a terminal failure: retry when transient, else Failed.
    fn handle_failure(&mut self, id: &str, now: Instant, err: TunnelError) {
        // Long-lived tunnel that died: fresh retry budget.
        let stable = self.runtimes[id]
            .incarnation_started
            .is_some_and(|t| now - t >= STABLE_UPTIME);
        if stable {
            self.runtimes
                .get_mut(id)
                .expect("runtime exists")
                .status
                .restart_count = 0;
        }
        let attempts = self.runtimes[id].status.restart_count;
        if is_transient(&err) && attempts < self.max_retries {
            let delay = self.retry_delays[(attempts as usize).min(self.retry_delays.len() - 1)];
            let rt = self.runtimes.get_mut(id).expect("runtime exists");
            rt.status.restart_count = attempts + 1;
            rt.next_retry_at = Some(now + delay);
            rt.status.last_error = Some(err.to_string());
            self.set_state(id, TunnelState::Failed);
            self.emit(
                id,
                ManagerEventKind::RetryScheduled {
                    attempt: attempts + 1,
                    delay_secs: delay.as_secs(),
                },
            );
        } else {
            self.fail_now(id, err);
        }
    }

    /// Immediate terminal failure: no retry. Returns the error for `start`
    /// to propagate; other callers use it as a statement.
    fn fail_now(&mut self, id: &str, err: TunnelError) -> TunnelError {
        {
            let rt = self.runtimes.get_mut(id).expect("runtime exists");
            rt.child = None;
            rt.next_retry_at = None;
            rt.status.state = TunnelState::Failed;
            rt.status.pid = None;
            rt.status.last_error = Some(err.to_string());
        }
        let msg = err.to_string();
        self.emit(id, ManagerEventKind::Failed { error: msg });
        self.emit(id, ManagerEventKind::StateChanged(TunnelState::Failed));
        err
    }

    pub fn status(&self, id: &str) -> Result<TunnelStatus, TunnelError> {
        self.runtimes
            .get(id)
            .map(|rt| rt.status.clone())
            .ok_or_else(|| TunnelError::NotFound(id.to_owned()))
    }

    /// Snapshot of every known runtime, sorted by id.
    pub fn statuses(&self) -> Vec<TunnelStatus> {
        let mut out: Vec<TunnelStatus> =
            self.runtimes.values().map(|rt| rt.status.clone()).collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Last `limit` log lines for a tunnel (oldest first).
    pub fn logs(&self, id: &str, limit: usize) -> Result<Vec<String>, TunnelError> {
        let rt = self
            .runtimes
            .get(id)
            .ok_or_else(|| TunnelError::NotFound(id.to_owned()))?;
        let skip = rt.logs.len().saturating_sub(limit);
        Ok(rt.logs.iter().skip(skip).cloned().collect())
    }

    /// Drain pending manager events (Tauri forwards them to the frontend).
    pub fn drain_events(&mut self) -> Vec<ManagerEvent> {
        self.events.drain(..).collect()
    }
}

/// Retry only transient network/server failures — never config errors.
fn is_transient(err: &TunnelError) -> bool {
    match err {
        TunnelError::Exited(_, _) => true,
        TunnelError::SpawnFailed(msg) => msg.starts_with("cannot reach"),
        _ => false,
    }
}

/// Bind-then-drop probe: fail fast when the local endpoint is taken or
/// unparseable, before spawning ssh.
fn check_port_free(config: &TunnelConfig) -> Result<(), TunnelError> {
    let endpoint = config.local_endpoint();
    let addr: std::net::SocketAddr = endpoint
        .parse()
        .map_err(|_| TunnelError::InvalidBindAddress)?;
    match std::net::TcpListener::bind(addr) {
        Ok(_) => Ok(()),
        Err(_) => Err(TunnelError::PortInUse(endpoint)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel::{TunnelInput, TunnelMode};
    use std::collections::VecDeque as Queue;

    fn local_config(id: &str, port: u16) -> TunnelConfig {
        let mut cfg = TunnelInput {
            name: format!("t-{id}"),
            mode: TunnelMode::Local,
            ssh_target: "dev-bastion".to_owned(),
            bind_address: None,
            local_port: port,
            remote_host: Some("db.internal".to_owned()),
            remote_port: Some(5432),
            identity_file: None,
            proxy_jump: None,
            auto_start: None,
            keep_alive_interval: None,
        }
        .into_config()
        .unwrap();
        cfg.id = id.to_owned();
        cfg
    }

    /// Scripted child: alive for `live_polls` polls, then exits with `code`.
    struct FakeChild {
        pid: u32,
        live_polls: i32,
        code: i32,
        pending_lines: Queue<String>,
        killed: bool,
    }

    impl ManagedChild for FakeChild {
        fn pid(&self) -> u32 {
            self.pid
        }
        fn is_alive(&mut self) -> Result<bool, TunnelError> {
            if self.killed {
                return Ok(false);
            }
            if self.live_polls > 0 {
                self.live_polls -= 1;
                return Ok(true);
            }
            Ok(false)
        }
        fn kill(&mut self) -> Result<(), TunnelError> {
            self.killed = true;
            Ok(())
        }
        fn reap(&mut self, _tail: usize) -> (Option<i32>, Vec<String>) {
            (Some(self.code), self.pending_lines.drain(..).collect())
        }
        fn drain_stderr(&mut self) -> Vec<String> {
            self.pending_lines.drain(..).collect()
        }
    }

    struct FakeSpawner {
        spawns: usize,
        fail_with: Option<TunnelError>,
        live_polls: i32,
        code: i32,
        stderr: Vec<String>,
        next_pid: u32,
    }

    impl FakeSpawner {
        fn ok() -> Self {
            Self {
                spawns: 0,
                fail_with: None,
                live_polls: i32::MAX / 2,
                code: 0,
                stderr: Vec::new(),
                next_pid: 1000,
            }
        }
    }

    impl Spawner for FakeSpawner {
        fn spawn(&mut self, _config: &TunnelConfig) -> Result<Box<dyn ManagedChild>, TunnelError> {
            if let Some(err) = self.fail_with.clone() {
                return Err(err);
            }
            self.spawns += 1;
            self.next_pid += 1;
            Ok(Box::new(FakeChild {
                pid: self.next_pid,
                live_polls: self.live_polls,
                code: self.code,
                pending_lines: Queue::from(self.stderr.clone()),
                killed: false,
            }))
        }
    }

    /// Scripted dial answers, consumed in order; repeats the last one.
    struct ScriptedHealth {
        answers: Vec<bool>,
    }

    impl HealthChecker for ScriptedHealth {
        fn check(&mut self, _endpoint: &str) -> bool {
            if self.answers.len() > 1 {
                self.answers.remove(0)
            } else {
                self.answers[0]
            }
        }
    }

    fn manager() -> TunnelManager<FakeSpawner> {
        manager_with_health(vec![true])
    }

    fn manager_with_health(answers: Vec<bool>) -> TunnelManager<FakeSpawner> {
        let mut m = TunnelManager::new(FakeSpawner::ok());
        m.set_startup_grace(Duration::ZERO);
        m.set_retry_delays(vec![Duration::ZERO]);
        m.set_health_checker(Box::new(ScriptedHealth { answers }));
        m.set_health_interval(Duration::ZERO);
        m
    }

    #[test]
    fn start_promotes_to_running_and_stop_reaps() {
        let mut m = manager();
        m.upsert_config(local_config("a", 54391));
        let st = m.start("a").unwrap();
        assert_eq!(st.state, TunnelState::Starting);
        assert!(st.pid.is_some());

        m.poll(Instant::now());
        assert_eq!(m.status("a").unwrap().state, TunnelState::Running);

        // Second start is rejected.
        assert_eq!(
            m.start("a").unwrap_err(),
            TunnelError::AlreadyRunning("a".to_owned())
        );

        let stopped = m.stop("a").unwrap();
        assert_eq!(stopped.state, TunnelState::Stopped);
        assert_eq!(stopped.pid, None);
        assert_eq!(
            m.stop("a").unwrap_err(),
            TunnelError::NotRunning("a".to_owned())
        );
    }

    #[test]
    fn unknown_id_errors() {
        let mut m = manager();
        assert!(matches!(
            m.start("ghost").unwrap_err(),
            TunnelError::NotFound(_)
        ));
        assert!(matches!(
            m.stop("ghost").unwrap_err(),
            TunnelError::NotFound(_)
        ));
    }

    #[test]
    fn occupied_port_fails_before_spawn() {
        let mut m = manager();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        m.upsert_config(local_config("busy", port));
        let err = m.start("busy").unwrap_err();
        assert!(matches!(err, TunnelError::PortInUse(_)));
        assert_eq!(m.spawner.spawns, 0);
    }

    #[test]
    fn transient_exit_retries_then_recovers() {
        let mut m = manager();
        // First incarnation dies at once with a network error: start reports
        // the failure but schedules a retry.
        m.spawner.live_polls = 0;
        m.spawner.code = 255;
        m.spawner.stderr = vec!["ssh: connect to host x port 22: Connection timed out".into()];
        m.upsert_config(local_config("flaky", 54392));
        let err = m.start("flaky").unwrap_err();
        assert!(err.to_string().contains("cannot reach"));

        let st = m.status("flaky").unwrap();
        assert_eq!(st.state, TunnelState::Failed);
        assert_eq!(st.restart_count, 1);
        assert!(st.last_error.as_deref().unwrap().contains("cannot reach"));

        // Next incarnation stays up: retry spawn happened on the next poll.
        m.spawner.live_polls = i32::MAX / 2;
        m.spawner.stderr.clear();
        m.poll(Instant::now());
        m.poll(Instant::now());
        let st = m.status("flaky").unwrap();
        assert_eq!(st.state, TunnelState::Running);
        assert!(m.spawner.spawns >= 2);
    }

    #[test]
    fn retries_exhaust_then_stay_failed() {
        let mut m = manager();
        m.set_max_retries(2);
        m.spawner.live_polls = 0;
        m.spawner.code = 255;
        m.spawner.stderr = vec!["Connection timed out".into()];
        m.upsert_config(local_config("down", 54393));
        // start() itself consumes the first attempt (1 initial spawn).
        m.start("down").unwrap_err();
        for _ in 0..10 {
            m.poll(Instant::now());
        }
        let st = m.status("down").unwrap();
        assert_eq!(st.state, TunnelState::Failed);
        assert_eq!(st.restart_count, 2);
        // 1 initial + 2 retries, no more.
        assert_eq!(m.spawner.spawns, 3);
    }

    #[test]
    fn config_errors_never_retry() {
        let mut m = manager();
        m.spawner.live_polls = 0;
        m.spawner.code = 255;
        m.spawner.stderr =
            vec!["channel_setup_fwd_listener_tcpip: cannot listen to port: 54394".into()];
        m.upsert_config(local_config("conflict", 54394));
        let err = m.start("conflict").unwrap_err();
        assert!(matches!(err, TunnelError::PortInUse(_)));
        m.poll(Instant::now());
        m.poll(Instant::now());
        let st = m.status("conflict").unwrap();
        assert_eq!(st.state, TunnelState::Failed);
        assert_eq!(st.restart_count, 0);
        assert_eq!(m.spawner.spawns, 1);
    }

    #[test]
    fn stop_cancels_pending_retry() {
        let mut m = manager();
        m.spawner.live_polls = 0;
        m.spawner.code = 255;
        m.spawner.stderr = vec!["Connection timed out".into()];
        m.upsert_config(local_config("s", 54395));
        // start() consumes attempt 1 (restart_count=1, retry due immediately).
        m.start("s").unwrap_err();
        assert_eq!(m.status("s").unwrap().restart_count, 1);
        m.poll(Instant::now());
        // Retry fired at once (ZERO delay): attempt 2 failed, retry 3 pending.
        assert_eq!(m.status("s").unwrap().restart_count, 2);
        // Failed-with-retry-pending is stoppable: treat as user cancel.
        let rt = m.runtimes.get_mut("s").expect("runtime exists");
        rt.stop_requested = true;
        rt.next_retry_at = None;
        m.set_state("s", TunnelState::Stopped);
        let spawns = m.spawner.spawns;
        m.poll(Instant::now());
        assert_eq!(m.spawner.spawns, spawns);
        assert_eq!(m.status("s").unwrap().state, TunnelState::Stopped);
    }

    #[test]
    fn stderr_flows_into_logs_and_events() {
        let mut m = manager();
        m.spawner.stderr = vec![
            "debug1: Connecting to x".into(),
            "debug1: Connected.".into(),
        ];
        m.upsert_config(local_config("loggy", 54396));
        m.start("loggy").unwrap();
        m.poll(Instant::now());
        let logs = m.logs("loggy", 10).unwrap();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].contains("Connecting"));
        let events = m.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, ManagerEventKind::Started { .. })));
        assert!(m.drain_events().is_empty());
    }

    #[test]
    fn remove_config_stops_running_tunnel() {
        let mut m = manager();
        m.upsert_config(local_config("tmp", 54397));
        m.start("tmp").unwrap();
        m.remove_config("tmp").unwrap();
        assert!(matches!(
            m.status("tmp").unwrap_err(),
            TunnelError::NotFound(_)
        ));
        assert!(m.config("tmp").is_none());
    }

    #[test]
    fn restart_restarts_cleanly() {
        let mut m = manager();
        m.upsert_config(local_config("r", 54398));
        m.start("r").unwrap();
        m.poll(Instant::now());
        let first_pid = m.status("r").unwrap().pid;
        m.restart("r").unwrap();
        m.poll(Instant::now());
        let second_pid = m.status("r").unwrap().pid;
        assert_eq!(m.status("r").unwrap().state, TunnelState::Running);
        assert_ne!(first_pid, second_pid);
    }

    #[test]
    fn unhealthy_promote_goes_degraded() {
        let mut m = manager_with_health(vec![false]);
        m.upsert_config(local_config("deg", 54401));
        m.start("deg").unwrap();
        m.poll(Instant::now());
        assert_eq!(m.status("deg").unwrap().state, TunnelState::Degraded);
        let events = m.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, ManagerEventKind::HealthChanged { healthy: false })));
    }

    #[test]
    fn degraded_recovers_to_running() {
        let mut m = manager_with_health(vec![false, true]);
        m.upsert_config(local_config("rec", 54402));
        m.start("rec").unwrap();
        m.poll(Instant::now());
        assert_eq!(m.status("rec").unwrap().state, TunnelState::Degraded);
        m.poll(Instant::now());
        assert_eq!(m.status("rec").unwrap().state, TunnelState::Running);
    }

    #[test]
    fn running_degrades_when_dial_fails() {
        let mut m = manager_with_health(vec![true, false]);
        m.upsert_config(local_config("wd", 54403));
        m.start("wd").unwrap();
        m.poll(Instant::now());
        assert_eq!(m.status("wd").unwrap().state, TunnelState::Running);
        m.poll(Instant::now());
        assert_eq!(m.status("wd").unwrap().state, TunnelState::Degraded);
    }

    #[test]
    fn throttles_rechecks_within_interval() {
        struct Counting {
            calls: usize,
        }
        impl HealthChecker for Counting {
            fn check(&mut self, _endpoint: &str) -> bool {
                self.calls += 1;
                true
            }
        }
        let mut m = TunnelManager::new(FakeSpawner::ok());
        m.set_startup_grace(Duration::ZERO);
        m.set_health_checker(Box::new(Counting { calls: 0 }));
        m.set_health_interval(Duration::from_secs(3600));
        m.upsert_config(local_config("th", 54404));
        m.start("th").unwrap();
        m.poll(Instant::now());
        m.poll(Instant::now());
        m.poll(Instant::now());
        // 1 promote dial + 1 interval recheck; throttled afterwards.
        assert_eq!(m.status("th").unwrap().state, TunnelState::Running);
    }
}
