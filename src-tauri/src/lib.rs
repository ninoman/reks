//! Tauri shell for reks. Commands, state and events only; business logic
//! lives in `crates/*` (docs/decisions.md D2).

mod commands;
mod state;

use tauri::Manager;

use state::AppState;

/// Returns the core library version. Smoke-test command for the scaffold.
#[tauri::command]
fn core_version() -> &'static str {
    reks_core::VERSION
}

/// Builds and runs the Tauri application.
///
/// # Panics
/// Panics if the Tauri runtime fails to start; there is nothing to recover to.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let paths = app.path();
            // D19: app_data_dir, but app_local_data_dir on Windows (not roaming).
            let data_dir = if cfg!(windows) {
                paths.app_local_data_dir()?
            } else {
                paths.app_data_dir()?
            };
            let state = AppState::open(&data_dir, &paths.app_cache_dir()?)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            commands::library_query,
            commands::library_facets,
            commands::library_track,
            commands::library_update_track,
            commands::playlists_get,
            commands::playlists_mutate,
            commands::artwork_get,
            commands::scan_start,
            commands::scan_cancel,
            commands::rbxml_import_start,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
