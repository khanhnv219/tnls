//! System tray: show/hide window, start-all/stop-all, graceful quit.
//!
//! - Close button asks the frontend: minimize to tray or quit (window event).
//! - Left click on the tray icon (or "Show tnls") restores the window.
//! - Tray title shows the live `N running` count (updated on every tick).
//! - Quit stops all tunnels first, then exits — no orphaned ssh.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, WindowEvent,
};

use crate::state::AppState;

const TRAY_ID: &str = "tnls-tray";

/// Menu item ids (stable contract between builder and handler).
const SHOW: &str = "tray-show";
const START_ALL: &str = "tray-start-all";
const STOP_ALL: &str = "tray-stop-all";
const QUIT: &str = "tray-quit";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, SHOW, "Show tnls", true, None::<&str>)?;
    let start_all = MenuItem::with_id(app, START_ALL, "Start all", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, STOP_ALL, "Stop all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit (stops all tunnels)", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &start_all, &stop_all, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("tnls — SSH tunnel manager")
        .menu(&menu)
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            SHOW => show_main(app),
            START_ALL => start_all_tunnels(app),
            STOP_ALL => stop_all_tunnels(app),
            QUIT => quit_gracefully(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::WebviewWindow<R>> {
    app.get_webview_window("main")
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = main_window(app) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn start_all_tunnels<R: Runtime>(app: &AppHandle<R>) {
    let (ids, results) = match app.try_state::<AppState>() {
        Some(state) => {
            let ids: Vec<String> = match state.db() {
                Ok(db) => tnls_core::persistence::list(&db)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| c.id)
                    .collect(),
                Err(e) => {
                    tracing::warn!("tray start-all: db lock failed: {e}");
                    return;
                }
            };
            let mut results = Vec::new();
            if let Ok(mut manager) = state.manager() {
                for id in &ids {
                    if manager.config(id).is_none() {
                        continue;
                    }
                    results.push((id.clone(), manager.start(id).map(|_| ())));
                }
            }
            (ids, results)
        }
        None => return,
    };
    for (id, result) in results {
        if let Err(e) = result {
            tracing::warn!("tray start-all {id}: {e}");
        }
    }
    let _ = app.emit(
        "tunnel://state-changed",
        serde_json::json!({"bulk": "start-all", "ids": ids}),
    );
    show_main(app);
}

fn stop_all_tunnels<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut manager) = state.manager() {
            manager.stop_all();
        }
    }
}

/// Stop every tunnel, then exit. Shared by the tray menu and the close dialog.
pub fn quit_gracefully<R: Runtime>(app: &AppHandle<R>) {
    tracing::info!("quit: stopping all tunnels");
    stop_all_tunnels(app);
    app.exit(0);
}

/// Event the main window receives when the user presses its close button.
pub const CLOSE_REQUESTED_EVENT: &str = "app://close-requested";

/// The close button never closes the window directly: the frontend asks
/// whether to minimize to the tray or quit (or applies the remembered
/// choice) and answers through `hide_to_tray` / `quit_app`.
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        if window.emit(CLOSE_REQUESTED_EVENT, ()).is_err() {
            // No webview to ask: fall back to the safe choice, keep tunnels up.
            let _ = window.hide();
        }
    }
}

/// Stop tunnels on any exit path (OS shutdown, `app.exit`), so no ssh
/// process outlives tnls. `stop_all` is a no-op when nothing runs.
pub fn on_exit<R: Runtime>(app: &AppHandle<R>) {
    stop_all_tunnels(app);
}

/// Refresh the tray title with the live running count. Called from the
/// supervisor tick (cheap: one lock + format).
pub fn update_count<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let running = app
        .try_state::<AppState>()
        .and_then(|state| {
            state.manager().ok().map(|manager| {
                manager
                    .statuses()
                    .iter()
                    .filter(|s| {
                        matches!(
                            s.state,
                            tnls_core::tunnel::TunnelState::Running
                                | tnls_core::tunnel::TunnelState::Starting
                        )
                    })
                    .count()
            })
        })
        .unwrap_or(0);
    let title = if running == 0 {
        "tnls".to_owned()
    } else {
        format!("tnls — {running} running")
    };
    let _ = tray.set_title(Some(title));
    let _ = tray.set_tooltip(Some(format!("tnls — {running} tunnel(s) running")));
}
