use tauri::State;
use tnls_core::logs::LogLine;
use tnls_core::persistence;
use tnls_core::tunnel::{TunnelConfig, TunnelError, TunnelInput, TunnelStatus};

use crate::state::{err_text, AppState};

#[tauri::command]
pub fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelConfig>, String> {
    let db = state.db()?;
    persistence::list(&db).map_err(err_text)
}

#[tauri::command]
pub fn create_tunnel(
    state: State<'_, AppState>,
    input: TunnelInput,
) -> Result<TunnelConfig, String> {
    let db = state.db()?;
    let config = persistence::create(&db, input).map_err(err_text)?;
    state.manager()?.upsert_config(config.clone());
    Ok(config)
}

#[tauri::command]
pub fn update_tunnel(
    state: State<'_, AppState>,
    id: String,
    input: TunnelInput,
) -> Result<TunnelConfig, String> {
    let db = state.db()?;
    let config = persistence::update(&db, &id, input).map_err(err_text)?;
    state.manager()?.upsert_config(config.clone());
    Ok(config)
}

#[tauri::command]
pub fn delete_tunnel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let db = state.db()?;
        persistence::delete(&db, &id).map_err(err_text)?;
    }
    let _ = tnls_core::keychain::delete_passphrase(&id);
    state.manager()?.remove_config(&id).map_err(err_text)?;
    Ok(())
}

/// Error shape for commands that start a tunnel: a port conflict carries
/// who holds the port so the UI can offer the right fix; everything else is
/// a message.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartError {
    #[serde(rename_all = "camelCase")]
    PortConflict {
        message: String,
        conflict: Box<tnls_core::ports::PortConflict>,
    },
    Other {
        message: String,
    },
}

impl From<String> for StartError {
    fn from(message: String) -> Self {
        Self::Other { message }
    }
}

impl From<TunnelError> for StartError {
    fn from(err: TunnelError) -> Self {
        let message = err.to_string();
        match err {
            TunnelError::PortConflict(conflict) => Self::PortConflict { message, conflict },
            _ => Self::Other { message },
        }
    }
}

#[tauri::command]
pub fn start_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelStatus, StartError> {
    start_inner(&state, &id)
}

fn start_inner(state: &AppState, id: &str) -> Result<TunnelStatus, StartError> {
    // Scope each lock separately: never hold the manager guard while
    // acquiring the db guard (Tauri runs commands concurrently).
    let managed = state.manager()?.config(id).is_some();
    if !managed {
        let config = {
            let db = state.db()?;
            persistence::get(&db, id)?
        };
        state.manager()?.upsert_config(config);
    }
    Ok(state.manager()?.start(id)?)
}

#[tauri::command]
pub fn stop_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelStatus, String> {
    state.manager()?.stop(&id).map_err(err_text)
}

#[tauri::command]
pub fn restart_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelStatus, StartError> {
    Ok(state.manager()?.restart(&id)?)
}

/// Conflict dialog: stop the tunnel holding the port, then start this one.
#[tauri::command]
pub fn take_over_port(
    state: State<'_, AppState>,
    holder_id: String,
    id: String,
) -> Result<TunnelStatus, StartError> {
    state.manager()?.stop(&holder_id)?;
    start_inner(&state, &id)
}

#[tauri::command]
pub fn get_tunnel_logs(
    state: State<'_, AppState>,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<LogLine>, String> {
    state
        .manager()?
        .logs(&id, limit.unwrap_or(200).min(1000))
        .map_err(err_text)
}

/// The ssh invocation a (possibly half-filled) form would produce.
#[derive(serde::Serialize)]
pub struct CommandPreview {
    /// Resolved ssh binary, e.g. `C:\Windows\System32\OpenSSH\ssh.exe`.
    program: String,
    /// Arguments after the program; missing values appear as `<placeholders>`.
    args: Vec<String>,
}

/// Display-only preview built by the same code that spawns ssh. Nothing is
/// stored or executed.
#[tauri::command]
pub fn preview_ssh_command(input: TunnelInput) -> CommandPreview {
    let draft = input.into_draft();
    CommandPreview {
        program: tnls_core::ssh::ssh_binary().to_string_lossy().into_owned(),
        args: tnls_core::ssh::preview_args(&draft),
    }
}

#[tauri::command]
pub fn get_runtime_status(state: State<'_, AppState>) -> Result<Vec<TunnelStatus>, String> {
    Ok(state.manager()?.statuses())
}

/// Close dialog: keep running in the background; reopen from the tray icon.
#[tauri::command]
pub fn hide_to_tray(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

/// Close dialog: stop every tunnel and exit.
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    crate::tray::quit_gracefully(&app);
}

#[tauri::command]
pub fn get_passphrase_state(id: String) -> Result<bool, String> {
    Ok(tnls_core::keychain::has_passphrase(&id))
}

#[tauri::command]
pub fn set_passphrase(id: String, passphrase: String) -> Result<(), String> {
    tnls_core::keychain::set_passphrase(&id, &passphrase).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_passphrase(id: String) -> Result<(), String> {
    tnls_core::keychain::delete_passphrase(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_tunnels_json(state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db()?;
    let configs = persistence::list(&db).map_err(err_text)?;
    let inputs: Vec<TunnelInput> = configs
        .into_iter()
        .map(|c| TunnelInput {
            name: c.name,
            mode: c.mode,
            ssh_target: c.ssh_target,
            bind_address: Some(c.bind_address),
            local_port: c.local_port,
            remote_host: c.remote_host,
            remote_port: c.remote_port,
            identity_file: c.identity_file,
            proxy_jump: c.proxy_jump,
            auto_start: Some(c.auto_start),
            keep_alive_interval: Some(c.keep_alive_interval),
            max_retries: Some(c.max_retries),
            retry_delay_secs: Some(c.retry_delay_secs),
            tags: Some(c.tags),
        })
        .collect();
    Ok(tnls_core::import::format_json_inputs(&inputs))
}

#[tauri::command]
pub fn import_tunnels_json(
    state: State<'_, AppState>,
    json: String,
) -> Result<Vec<TunnelConfig>, String> {
    let inputs = tnls_core::import::parse_json_inputs(&json).map_err(|e| e.to_string())?;
    let mut created = Vec::new();
    {
        let db = state.db()?;
        let mut manager = state.manager()?;
        for input in inputs {
            let config = persistence::create(&db, input).map_err(err_text)?;
            manager.upsert_config(config.clone());
            created.push(config);
        }
    }
    Ok(created)
}

#[tauri::command]
pub fn import_ssh_config(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<Vec<TunnelConfig>, String> {
    let p = path.as_deref().map(std::path::Path::new);
    let hosts = tnls_core::import::import_ssh_config(p).map_err(|e| e.to_string())?;
    let mut created = Vec::new();
    {
        let db = state.db()?;
        let mut manager = state.manager()?;
        for host in hosts {
            for input in tnls_core::import::host_to_inputs(&host) {
                match persistence::create(&db, input) {
                    Ok(cfg) => {
                        manager.upsert_config(cfg.clone());
                        created.push(cfg);
                    }
                    Err(e) => tracing::warn!("ssh import skip: {}", e),
                }
            }
        }
    }
    Ok(created)
}
