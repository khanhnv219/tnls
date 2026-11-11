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
use crate::logs::{LogLevel, LogLine};
use crate::ports::{ConflictOwner, OsPortProbe, PortConflict, PortProbe};
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
/// How long a Starting tunnel may wait for ssh to open its local listener
/// (connect + auth, possibly through a jump host) before it counts as Degraded.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
/// Re-dial cadence for the local listener while Starting.
const STARTUP_DIAL_INTERVAL: Duration = Duration::from_millis(500);
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
    /// Blocking reap: wait for exit, return code + the stderr lines that
    /// `drain_stderr` has not returned yet (at most `tail`).
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

    /// Returns only lines not yet handed out by `drain_stderr`, so callers
    /// can append them to the log without duplicating earlier output.
    fn reap(&mut self, tail: usize) -> (Option<i32>, Vec<String>) {
        let Some(child) = self.child.take() else {
            return (None, Vec::new());
        };
        let code = child.wait();
        let rest: Vec<String> = self.rx.try_iter().collect();
        let start = rest.len().saturating_sub(tail);
        (code, rest[start..].to_vec())
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
    logs: VecDeque<LogLine>,
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

    /// Append stderr lines, stamped with the collection time and a level.
    fn push_logs(&mut self, lines: Vec<String>) {
        let at = Utc::now();
        for line in lines {
            self.push_line(LogLine::new(at, line));
        }
    }

    fn push_line(&mut self, line: LogLine) {
        if self.logs.len() >= LOG_CAP {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }
}

/// Supervisor for all tunnel processes.
pub struct TunnelManager<S: Spawner = SshSpawner> {
    spawner: S,
    checker: Box<dyn HealthChecker>,
    ports: Box<dyn PortProbe>,
    configs: HashMap<String, TunnelConfig>,
    runtimes: HashMap<String, Runtime>,
    events: VecDeque<ManagerEvent>,
    retry_delays: Vec<Duration>,
    max_retries: u32,
    startup_grace: Duration,
    /// How long a Starting tunnel may wait for its local listener.
    startup_timeout: Duration,
    /// Re-dial cadence while Starting.
    startup_dial_interval: Duration,
    health_interval: Duration,
}

impl<S: Spawner> TunnelManager<S> {
    pub fn new(spawner: S) -> Self {
        Self {
            spawner,
            checker: Box::new(TcpHealthChecker),
            ports: Box::new(OsPortProbe),
            configs: HashMap::new(),
            runtimes: HashMap::new(),
            events: VecDeque::new(),
            retry_delays: DEFAULT_RETRY_DELAYS.to_vec(),
            max_retries: DEFAULT_MAX_RETRIES,
            startup_grace: Duration::from_millis(STARTUP_GRACE_MS),
            startup_timeout: STARTUP_TIMEOUT,
            startup_dial_interval: STARTUP_DIAL_INTERVAL,
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

    /// How long Starting may wait for the local listener before Degraded.
    pub fn set_startup_timeout(&mut self, timeout: Duration, dial_interval: Duration) {
        self.startup_timeout = timeout;
        self.startup_dial_interval = dial_interval;
    }

    fn emit(&mut self, id: &str, kind: ManagerEventKind) {
        let at = Utc::now();
        self.log_event(id, &kind, at);
        self.events.push_back(ManagerEvent {
            id: id.to_owned(),
            kind,
            at,
        });
    }

    /// Mirror lifecycle events into the tunnel's log. A healthy ssh with
    /// BatchMode writes nothing to stderr, so without these lines the log
    /// stays empty while desktop notifications report what happened.
    fn log_event(&mut self, id: &str, kind: &ManagerEventKind, at: DateTime<Utc>) {
        let (endpoint, remote) = match self.configs.get(id) {
            Some(c) => (
                c.local_endpoint(),
                c.mode == crate::tunnel::TunnelMode::Remote,
            ),
            None => (String::new(), false),
        };
        let line = match kind {
            ManagerEventKind::Started { pid } => Some((
                LogLevel::Info,
                if remote {
                    format!("ssh started (pid {pid})")
                } else {
                    format!("ssh started (pid {pid}), waiting for {endpoint} to accept connections")
                },
            )),
            ManagerEventKind::StateChanged(TunnelState::Running) => Some((
                LogLevel::Info,
                if remote {
                    "running: ssh process is up".to_owned()
                } else {
                    format!("running: {endpoint} is accepting connections")
                },
            )),
            // Other transitions are covered by their own events below.
            ManagerEventKind::StateChanged(_) => None,
            ManagerEventKind::HealthChanged { healthy: false } => Some((
                LogLevel::Warn,
                format!("degraded: ssh is alive but {endpoint} is not accepting connections"),
            )),
            // Recovery is logged by the StateChanged(Running) that precedes it.
            ManagerEventKind::HealthChanged { healthy: true } => None,
            ManagerEventKind::RetryScheduled {
                attempt,
                delay_secs,
            } => Some((
                LogLevel::Warn,
                format!("retry {attempt} scheduled in {delay_secs}s"),
            )),
            ManagerEventKind::Failed { error } => {
                Some((LogLevel::Error, format!("failed: {error}")))
            }
            ManagerEventKind::Stopped => Some((LogLevel::Info, "stopped".to_owned())),
        };
        if let (Some((level, message)), Some(rt)) = (line, self.runtimes.get_mut(id)) {
            rt.push_line(LogLine::tnls(at, level, &message));
        }
    }

    fn set_state(&mut self, id: &str, state: TunnelState) {
        if let Some(rt) = self.runtimes.get_mut(id) {
            rt.status.state = state;
        }
        self.emit(id, ManagerEventKind::StateChanged(state));
    }

    /// Starting → Running once the local endpoint accepts a connection.
    ///
    /// OpenSSH binds `-L`/`-D` listeners only after the connection and
    /// authentication finish, which usually takes longer than the startup
    /// grace. So a closed port while Starting is expected: re-dial every
    /// `startup_dial_interval` and only report Degraded after
    /// `startup_timeout` without a listener.
    fn probe_while_starting(&mut self, id: &str, now: Instant) {
        let (due, waited) = {
            let rt = &self.runtimes[id];
            let due = rt
                .last_health_check
                .is_none_or(|t| now - t >= self.startup_dial_interval);
            let waited = rt
                .incarnation_started
                .map_or(Duration::ZERO, |t| now.saturating_duration_since(t));
            (due, waited)
        };
        if !due {
            return;
        }
        let Some(endpoint) = self.configs.get(id).map(TunnelConfig::local_endpoint) else {
            return;
        };
        let healthy = self.checker.check(&endpoint);
        {
            let rt = self.runtimes.get_mut(id).expect("runtime exists");
            rt.last_health_check = Some(now);
            rt.last_healthy = Some(healthy);
        }
        if healthy {
            self.set_state(id, TunnelState::Running);
        } else if waited >= self.startup_timeout {
            self.set_state(id, TunnelState::Degraded);
            self.emit(id, ManagerEventKind::HealthChanged { healthy: false });
        }
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

    /// Refuse to start `id` when its local endpoint is taken: first by another
    /// managed tunnel that currently has an ssh process (same port, binds
    /// overlap), then by anything else on the OS. A conflict is a config
    /// error: never retried.
    pub fn check_conflict(&mut self, id: &str) -> Result<(), TunnelError> {
        let config = self
            .configs
            .get(id)
            .ok_or_else(|| TunnelError::NotFound(id.to_owned()))?;
        if !listens_locally(config) {
            return Ok(());
        }
        let (bind, port) = (config.bind_address.clone(), config.local_port);
        if crate::ports::socket_addr(&bind, port).is_none() {
            return Err(TunnelError::InvalidBindAddress);
        }
        let managed = self.configs.values().find(|other| {
            other.id != id
                && listens_locally(other)
                && other.local_port == port
                && crate::ports::binds_overlap(&other.bind_address, &bind)
                && self
                    .runtimes
                    .get(&other.id)
                    .is_some_and(|rt| rt.child.is_some())
        });
        let owner = if let Some(other) = managed {
            Some(ConflictOwner::Tunnel {
                id: other.id.clone(),
                name: other.name.clone(),
                tags: other.tags.clone(),
                pid: self.runtimes.get(&other.id).and_then(|rt| rt.status.pid),
            })
        } else if self.ports.is_bindable(&bind, port) {
            None
        } else {
            Some(self.identify_owner(port))
        };
        match owner {
            None => Ok(()),
            Some(owner) => Err(TunnelError::PortConflict(Box::new(PortConflict {
                bind,
                port,
                owner,
            }))),
        }
    }

    /// Name whoever listens on `port`: a managed tunnel when the OS reports
    /// one of our ssh pids, otherwise the OS process (or unknown).
    fn identify_owner(&mut self, port: u16) -> ConflictOwner {
        let listener = self.ports.find_listener(port);
        let pid = listener.as_ref().and_then(|l| l.pid);
        if let Some(pid) = pid {
            if let Some((id, _)) = self
                .runtimes
                .iter()
                .find(|(_, rt)| rt.status.pid == Some(pid) && rt.child.is_some())
            {
                if let Some(cfg) = self.configs.get(id) {
                    return ConflictOwner::Tunnel {
                        id: cfg.id.clone(),
                        name: cfg.name.clone(),
                        tags: cfg.tags.clone(),
                        pid: Some(pid),
                    };
                }
            }
        }
        ConflictOwner::Process {
            pid,
            process: listener.and_then(|l| l.process),
        }
    }

    /// Turn ssh's own "address already in use" (a race the pre-check missed)
    /// into the same structured conflict the pre-check reports.
    fn explain_port_error(&mut self, id: &str, err: TunnelError) -> TunnelError {
        if !matches!(err, TunnelError::PortInUse(_)) {
            return err;
        }
        let Some((bind, port)) = self
            .configs
            .get(id)
            .map(|c| (c.bind_address.clone(), c.local_port))
        else {
            return err;
        };
        let owner = self.identify_owner(port);
        TunnelError::PortConflict(Box::new(PortConflict { bind, port, owner }))
    }

    /// Note a refused start in the tunnel's log (no state change, no event).
    fn log_blocked_start(&mut self, id: &str, err: &TunnelError) {
        let rt = self
            .runtimes
            .entry(id.to_owned())
            .or_insert_with(|| Runtime::fresh(id));
        rt.push_line(LogLine::tnls(
            Utc::now(),
            LogLevel::Error,
            &format!("start blocked: {err}"),
        ));
    }

    /// Override the OS port probe (tests inject scripted answers).
    pub fn set_port_probe(&mut self, probe: Box<dyn PortProbe>) {
        self.ports = probe;
    }

    /// Diagnostics stage 2, read-only: is the local endpoint usable? When
    /// this tunnel is running the port is expectedly held by its own ssh,
    /// so the question becomes whether it accepts connections.
    pub fn local_forward_verdict(&mut self, id: &str) -> crate::diagnose::Verdict {
        use crate::diagnose::Outcome;
        let Some(config) = self.configs.get(id) else {
            return (Outcome::Fail, format!("tunnel {id} not found"));
        };
        let endpoint = config.local_endpoint();
        let own_pid = self
            .runtimes
            .get(id)
            .filter(|rt| rt.child.is_some())
            .and_then(|rt| rt.status.pid);
        if let Some(pid) = own_pid {
            return if self.checker.check(&endpoint) {
                (
                    Outcome::Pass,
                    format!(
                        "{endpoint} is held by this tunnel (pid {pid}) and accepting connections"
                    ),
                )
            } else {
                (
                    Outcome::Fail,
                    format!("{endpoint} is held by this tunnel (pid {pid}) but not accepting connections"),
                )
            };
        }
        match self.check_conflict(id) {
            Ok(()) => (
                Outcome::Pass,
                format!("{endpoint} is free and can be bound"),
            ),
            Err(err) => (Outcome::Fail, err.to_string()),
        }
    }

    /// Append a tnls line to a tunnel's log without touching its state.
    pub fn append_log(&mut self, id: &str, line: LogLine) {
        self.runtimes
            .entry(id.to_owned())
            .or_insert_with(|| Runtime::fresh(id))
            .push_line(line);
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
        // A taken port is a config error: refuse without touching the
        // tunnel's state and without scheduling a retry.
        if let Err(err) = self.check_conflict(id) {
            self.log_blocked_start(id, &err);
            return Err(err);
        }
        let mut child = self.spawner.spawn(&config)?;
        // Reap instant-exit failures (bad config surfaces fast thanks to
        // ExitOnForwardFailure) instead of reporting a phantom Running.
        // Route through the retry policy: transient errors get retries,
        // config errors fail immediately.
        if !child.is_alive()? {
            let (code, tail) = child.reap(FAILURE_TAIL);
            let err = ssh::classify_exit(code, &tail);
            let err = self.explain_port_error(id, err);
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
            if let Err(err) = self.check_conflict(&id) {
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
                            self.probe_while_starting(&id, now);
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
                    let err = self.explain_port_error(&id, err);
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
        // Per-tunnel retry policy overrides global defaults when not in test-zero mode.
        let use_test_override = self.retry_delays.contains(&Duration::ZERO);
        let (max_retries, delay) = if use_test_override {
            let d = self.retry_delays[(attempts as usize).min(self.retry_delays.len() - 1)];
            (self.max_retries, d)
        } else if let Some(cfg) = self.configs.get(id) {
            let base = cfg.retry_delay_secs;
            let cap = 30u64;
            let secs = base.saturating_mul(1u64 << attempts).min(cap);
            (cfg.max_retries, Duration::from_secs(secs))
        } else {
            let d = self.retry_delays[(attempts as usize).min(self.retry_delays.len() - 1)];
            (self.max_retries, d)
        };
        if is_transient(&err) && attempts < max_retries {
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
    pub fn logs(&self, id: &str, limit: usize) -> Result<Vec<LogLine>, TunnelError> {
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

/// Whether a tunnel opens a listener on this machine. `-R` listens on the
/// SSH server, so its bind/local port are never probed locally.
fn listens_locally(config: &TunnelConfig) -> bool {
    config.mode != crate::tunnel::TunnelMode::Remote
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel::{TunnelInput, TunnelMode};
    use std::collections::VecDeque as Queue;

    /// A port that is bindable right now. Fixed ports break on Windows hosts
    /// where Hyper-V/WinNAT reserve dynamic ranges (`netsh int ipv4 show
    /// excludedportrange protocol=tcp`), because `start` probes the bind.
    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        listener.local_addr().expect("local addr").port()
    }

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
            max_retries: None,
            retry_delay_secs: None,
            tags: None,
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

    /// Scripted OS: ports in `busy` cannot be bound and report `owner`.
    #[derive(Default)]
    struct FakePorts {
        busy: Vec<(u16, Option<crate::ports::Listener>)>,
    }

    impl PortProbe for FakePorts {
        fn is_bindable(&mut self, _bind: &str, port: u16) -> bool {
            !self.busy.iter().any(|(p, _)| *p == port)
        }
        fn find_listener(&mut self, port: u16) -> Option<crate::ports::Listener> {
            self.busy
                .iter()
                .find(|(p, _)| *p == port)
                .and_then(|(_, l)| l.clone())
        }
    }

    fn manager_with_health(answers: Vec<bool>) -> TunnelManager<FakeSpawner> {
        let mut m = TunnelManager::new(FakeSpawner::ok());
        m.set_port_probe(Box::new(FakePorts::default()));
        m.set_startup_grace(Duration::ZERO);
        m.set_startup_timeout(Duration::ZERO, Duration::ZERO);
        m.set_retry_delays(vec![Duration::ZERO]);
        m.set_health_checker(Box::new(ScriptedHealth { answers }));
        m.set_health_interval(Duration::ZERO);
        m
    }

    fn config_at(id: &str, bind: &str, port: u16) -> TunnelConfig {
        let mut cfg = local_config(id, port);
        cfg.bind_address = bind.to_owned();
        cfg.name = format!("tunnel {id}");
        cfg
    }

    #[test]
    fn managed_tunnel_on_same_bind_and_port_blocks_start() {
        let mut m = manager();
        let port = free_port();
        let mut a = config_at("a", "127.0.0.1", port);
        a.tags = vec!["homelab".into()];
        m.upsert_config(a);
        m.upsert_config(config_at("b", "127.0.0.1", port));
        m.start("a").unwrap();
        m.poll(Instant::now());
        let spawns = m.spawner.spawns;

        let err = m.start("b").unwrap_err();
        let TunnelError::PortConflict(conflict) = err else {
            panic!("expected PortConflict, got {err:?}");
        };
        assert_eq!((conflict.bind.as_str(), conflict.port), ("127.0.0.1", port));
        match &conflict.owner {
            ConflictOwner::Tunnel {
                id,
                name,
                tags,
                pid,
            } => {
                assert_eq!((id.as_str(), name.as_str()), ("a", "tunnel a"));
                assert_eq!(tags, &["homelab"]);
                assert!(pid.is_some());
            }
            other => panic!("expected managed owner, got {other:?}"),
        }
        // Blocked means blocked: no spawn, no retry, A untouched, B unchanged.
        assert_eq!(m.spawner.spawns, spawns);
        assert_eq!(m.status("a").unwrap().state, TunnelState::Running);
        assert!(m
            .status("b")
            .map_or(true, |s| s.state == TunnelState::Stopped));
        m.poll(Instant::now());
        assert_eq!(m.spawner.spawns, spawns);
        assert!(m
            .logs("b", 10)
            .unwrap()
            .iter()
            .any(|l| l.text.starts_with("tnls: start blocked:") && l.level == LogLevel::Error));
    }

    #[test]
    fn wildcard_bind_conflicts_with_loopback_on_same_port() {
        let mut m = manager();
        let port = free_port();
        m.upsert_config(config_at("a", "0.0.0.0", port));
        m.upsert_config(config_at("b", "127.0.0.1", port));
        m.start("a").unwrap();
        assert!(matches!(m.start("b"), Err(TunnelError::PortConflict(_))));
    }

    #[test]
    fn same_port_on_a_different_bind_is_not_a_conflict() {
        let mut m = manager();
        let port = free_port();
        m.upsert_config(config_at("a", "127.0.0.1", port));
        m.upsert_config(config_at("b", "127.0.0.2", port));
        m.start("a").unwrap();
        m.start("b").unwrap();
    }

    #[test]
    fn different_port_on_the_same_bind_is_not_a_conflict() {
        let mut m = manager();
        m.upsert_config(config_at("a", "127.0.0.1", free_port()));
        m.upsert_config(config_at("b", "127.0.0.1", free_port()));
        m.start("a").unwrap();
        m.start("b").unwrap();
    }

    #[test]
    fn stopped_managed_tunnel_on_same_port_does_not_block() {
        let mut m = manager();
        let port = free_port();
        m.upsert_config(config_at("a", "127.0.0.1", port));
        m.upsert_config(config_at("b", "127.0.0.1", port));
        m.start("b").unwrap();
    }

    #[test]
    fn unmanaged_listener_is_reported_with_its_process() {
        let mut m = manager();
        let port = free_port();
        m.set_port_probe(Box::new(FakePorts {
            busy: vec![(
                port,
                Some(crate::ports::Listener {
                    pid: Some(4321),
                    process: Some("node.exe".into()),
                }),
            )],
        }));
        m.upsert_config(config_at("b", "127.0.0.1", port));
        let err = m.start("b").unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("127.0.0.1:{port} is already in use by node.exe (pid 4321)")
        );
        assert_eq!(m.spawner.spawns, 0);
    }

    #[test]
    fn unidentified_listener_is_an_unknown_process() {
        let mut m = manager();
        let port = free_port();
        m.set_port_probe(Box::new(FakePorts {
            busy: vec![(port, None)],
        }));
        m.upsert_config(config_at("b", "127.0.0.1", port));
        let err = m.start("b").unwrap_err();
        assert!(err.to_string().ends_with("an unknown process"), "{err}");
    }

    #[test]
    fn closed_port_while_starting_waits_instead_of_degrading() {
        // ssh is still authenticating: the first dials fail, then it binds.
        let mut m = manager_with_health(vec![false, false, true]);
        m.set_startup_timeout(Duration::from_secs(20), Duration::ZERO);
        m.upsert_config(local_config("slow", free_port()));
        m.start("slow").unwrap();
        m.poll(Instant::now());
        m.poll(Instant::now());
        assert_eq!(m.status("slow").unwrap().state, TunnelState::Starting);
        m.poll(Instant::now());
        assert_eq!(m.status("slow").unwrap().state, TunnelState::Running);
        let events = m.drain_events();
        assert!(!events
            .iter()
            .any(|e| matches!(e.kind, ManagerEventKind::HealthChanged { .. })));
        assert!(!m
            .logs("slow", 50)
            .unwrap()
            .iter()
            .any(|l| l.text.starts_with("tnls: degraded")));
    }

    #[test]
    fn listener_that_never_opens_degrades_after_timeout() {
        let mut m = manager_with_health(vec![false]);
        m.set_startup_timeout(Duration::from_secs(20), Duration::ZERO);
        m.upsert_config(local_config("never", free_port()));
        m.start("never").unwrap();
        let t0 = Instant::now();
        m.poll(t0);
        assert_eq!(m.status("never").unwrap().state, TunnelState::Starting);
        m.poll(t0 + Duration::from_secs(21));
        assert_eq!(m.status("never").unwrap().state, TunnelState::Degraded);
    }

    #[test]
    fn start_promotes_to_running_and_stop_reaps() {
        let mut m = manager();
        m.upsert_config(local_config("a", free_port()));
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
        // Real OS probe: a port held by this test process.
        let mut m = manager();
        m.set_port_probe(Box::new(OsPortProbe));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        m.upsert_config(local_config("busy", port));
        let err = m.start("busy").unwrap_err();
        assert!(matches!(err, TunnelError::PortConflict(_)), "{err:?}");
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
        m.upsert_config(local_config("flaky", free_port()));
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
        m.upsert_config(local_config("down", free_port()));
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
        let port = free_port();
        m.spawner.stderr = vec![format!(
            "channel_setup_fwd_listener_tcpip: cannot listen to port: {port}"
        )];
        m.upsert_config(local_config("conflict", port));
        let err = m.start("conflict").unwrap_err();
        // ssh's own "cannot listen" (a race the pre-check missed) becomes the
        // same structured conflict, and leaves the tunnel Failed with it.
        assert!(matches!(err, TunnelError::PortConflict(_)), "{err:?}");
        let status = m.status("conflict").unwrap();
        assert_eq!(status.state, TunnelState::Failed);
        assert!(status.last_error.unwrap().contains("already in use by"));
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
        m.upsert_config(local_config("s", free_port()));
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
        m.upsert_config(local_config("loggy", free_port()));
        m.start("loggy").unwrap();
        m.poll(Instant::now());
        let logs = m.logs("loggy", 10).unwrap();
        let ssh: Vec<_> = logs
            .iter()
            .filter(|l| !l.text.starts_with("tnls:"))
            .collect();
        assert_eq!(ssh.len(), 2);
        assert!(ssh[0].text.contains("Connecting"));
        assert_eq!(ssh[0].level, LogLevel::Debug);
        let events = m.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, ManagerEventKind::Started { .. })));
        assert!(m.drain_events().is_empty());
    }

    #[test]
    fn lifecycle_events_are_written_to_the_log() {
        // Silent ssh (the normal BatchMode case): the log must still explain
        // start, degrade, recovery and stop.
        let mut m = manager_with_health(vec![true, false, true]);
        m.upsert_config(local_config("life", free_port()));
        m.start("life").unwrap();
        m.poll(Instant::now()); // promote: healthy -> Running
        m.poll(Instant::now()); // recheck: unhealthy -> Degraded
        m.poll(Instant::now()); // recheck: healthy -> Running
        m.stop("life").unwrap();

        let logs = m.logs("life", 50).unwrap();
        let texts: Vec<&str> = logs.iter().map(|l| l.text.as_str()).collect();
        assert!(texts[0].starts_with("tnls: ssh started (pid"), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("tnls: running:")));
        let degraded = logs
            .iter()
            .find(|l| l.text.starts_with("tnls: degraded:"))
            .expect("degraded line");
        assert_eq!(degraded.level, LogLevel::Warn);
        assert_eq!(
            texts
                .iter()
                .filter(|t| t.starts_with("tnls: running:"))
                .count(),
            2
        );
        assert_eq!(texts.last(), Some(&"tnls: stopped"));
    }

    #[test]
    fn failures_and_retries_are_written_to_the_log() {
        let mut m = manager();
        m.spawner.live_polls = 0;
        m.spawner.code = 255;
        m.spawner.stderr = vec!["ssh: connect to host x port 22: Connection timed out".into()];
        m.upsert_config(local_config("flog", free_port()));
        m.start("flog").unwrap_err();

        let logs = m.logs("flog", 50).unwrap();
        assert!(logs
            .iter()
            .any(|l| l.text.starts_with("ssh: connect") && l.level == LogLevel::Error));
        assert!(logs
            .iter()
            .any(|l| l.text.starts_with("tnls: retry 1 scheduled") && l.level == LogLevel::Warn));
    }

    #[test]
    fn remove_config_stops_running_tunnel() {
        let mut m = manager();
        m.upsert_config(local_config("tmp", free_port()));
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
        m.upsert_config(local_config("r", free_port()));
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
        m.upsert_config(local_config("deg", free_port()));
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
        m.upsert_config(local_config("rec", free_port()));
        m.start("rec").unwrap();
        m.poll(Instant::now());
        assert_eq!(m.status("rec").unwrap().state, TunnelState::Degraded);
        m.poll(Instant::now());
        assert_eq!(m.status("rec").unwrap().state, TunnelState::Running);
    }

    #[test]
    fn running_degrades_when_dial_fails() {
        let mut m = manager_with_health(vec![true, false]);
        m.upsert_config(local_config("wd", free_port()));
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
        m.upsert_config(local_config("th", free_port()));
        m.start("th").unwrap();
        m.poll(Instant::now());
        m.poll(Instant::now());
        m.poll(Instant::now());
        // 1 promote dial + 1 interval recheck; throttled afterwards.
        assert_eq!(m.status("th").unwrap().state, TunnelState::Running);
    }
}
