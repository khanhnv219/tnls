//! Shared application state: SQLite connection + tunnel supervisor.
//!
//! Both are synchronous (`rusqlite`, blocking `poll`), so they live behind
//! plain [`Mutex`]es. Tauri commands are short critical sections: mutate,
//! clone the snapshot out, drop the guard, then emit.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tnls_core::manager::{ManagerEventKind, SshSpawner, TunnelManager};
use tnls_core::persistence::DbConnection;
use tnls_core::{persistence, tunnel::TunnelError};

/// Poll interval for the supervisor tick.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct AppState {
    db: Mutex<DbConnection>,
    manager: Mutex<TunnelManager<SshSpawner>>,
}

impl AppState {
    pub fn db(&self) -> Result<MutexGuard<'_, DbConnection>, String> {
        self.db
            .lock()
            .map_err(|e| format!("database lock poisoned: {e}"))
    }

    pub fn manager(&self) -> Result<MutexGuard<'_, TunnelManager<SshSpawner>>, String> {
        self.manager
            .lock()
            .map_err(|e| format!("manager lock poisoned: {e}"))
    }

    /// Load every stored profile into the manager, then start the ones
    /// flagged `auto_start`. Returns (loaded, started, failed).
    fn load_profiles(&self) -> Result<(usize, usize, usize), String> {
        let db = self.db()?;
        let configs = persistence::list(&db).map_err(|e| e.to_string())?;
        let mut manager = self.manager()?;
        let (mut started, mut failed) = (0, 0);
        for config in &configs {
            manager.upsert_config(config.clone());
            if config.auto_start {
                match manager.start(&config.id) {
                    Ok(_) => started += 1,
                    Err(e) => {
                        tracing::warn!(tunnel = %config.name, "auto-start failed: {e}");
                        failed += 1;
                    }
                }
            }
        }
        Ok((configs.len(), started, failed))
    }

    /// One supervisor tick + forward new events to all windows.
    fn tick_and_emit(&self, app: &AppHandle) {
        let events = {
            let Ok(mut manager) = self.manager() else {
                return;
            };
            manager.poll(Instant::now());
            manager.drain_events()
        };
        for event in events {
            let topic = match &event.kind {
                ManagerEventKind::StateChanged(_) => "tunnel://state-changed",
                ManagerEventKind::Started { .. } => "tunnel://started",
                ManagerEventKind::RetryScheduled { .. } => "tunnel://retry-scheduled",
                ManagerEventKind::Failed { .. } => "tunnel://failed",
                ManagerEventKind::Stopped => "tunnel://stopped",
                ManagerEventKind::HealthChanged { .. } => "tunnel://health-changed",
            };
            let _ = app.emit(topic, &event);
        }
        crate::tray::update_count(app);
    }
}

fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(dir.join("tunnels.db"))
}

/// Build state, load profiles, spawn the supervisor thread.
pub fn init(app: &AppHandle) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = persistence::open(&path).map_err(|e| e.to_string())?;
    let state = AppState {
        db: Mutex::new(conn),
        manager: Mutex::new(TunnelManager::new(SshSpawner)),
    };
    let (loaded, started, failed) = state.load_profiles()?;
    tracing::info!(db = %path.display(), profiles = loaded, auto_started = started, auto_start_failed = failed, "tnls state ready");
    app.manage(state);

    let handle = app.clone();
    std::thread::Builder::new()
        .name("tnls-supervisor".into())
        .spawn(move || loop {
            std::thread::sleep(POLL_INTERVAL);
            if let Some(state) = handle.try_state::<AppState>() {
                state.tick_and_emit(&handle);
            } else {
                break;
            }
        })
        .map_err(|e| format!("spawn supervisor thread: {e}"))?;
    Ok(())
}

pub fn err_text(err: TunnelError) -> String {
    err.to_string()
}
