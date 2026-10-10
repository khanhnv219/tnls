use tauri::State;
use tnls_core::persistence;
use tnls_core::tunnel::{TunnelConfig, TunnelInput, TunnelStatus};

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

#[tauri::command]
pub fn start_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelStatus, String> {
    // Scope each lock separately: never hold the manager guard while
    // acquiring the db guard (Tauri runs commands concurrently).
    let managed = state.manager()?.config(&id).is_some();
    if !managed {
        let config = {
            let db = state.db()?;
            persistence::get(&db, &id).map_err(err_text)?
        };
        state.manager()?.upsert_config(config);
    }
    state.manager()?.start(&id).map_err(err_text)
}

#[tauri::command]
pub fn stop_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelStatus, String> {
    state.manager()?.stop(&id).map_err(err_text)
}

#[tauri::command]
pub fn restart_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelStatus, String> {
    state.manager()?.restart(&id).map_err(err_text)
}

#[tauri::command]
pub fn get_tunnel_logs(
    state: State<'_, AppState>,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    state
        .manager()?
        .logs(&id, limit.unwrap_or(200).min(1000))
        .map_err(err_text)
}

#[tauri::command]
pub fn get_runtime_status(state: State<'_, AppState>) -> Result<Vec<TunnelStatus>, String> {
    Ok(state.manager()?.statuses())
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
