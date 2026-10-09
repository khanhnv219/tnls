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
