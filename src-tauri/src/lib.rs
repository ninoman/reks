//! Tauri shell for reks. Commands, state and events only; business logic
//! lives in `crates/*` (docs/decisions.md D2).

mod adopt;
mod analysis;
mod commands;
mod history;
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
        // Restores each window's size, position and maximised state on start and
        // saves it on close, in `app_config_dir/.window-state.json`.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let paths = app.path();
            // D19: app_data_dir, but app_local_data_dir on Windows (not roaming).
            let data_dir = if cfg!(windows) {
                paths.app_local_data_dir()?
            } else {
                paths.app_data_dir()?
            };
            let model_dir = analysis::model_dir(paths.resource_dir().ok().as_deref());
            let state = AppState::open(&data_dir, &paths.app_cache_dir()?, &model_dir)?;
            // Tracks added or changed while the app was closed, or analysed
            // by an older analyser version.
            state.analysis.queue_pending();
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
            commands::rbxml_preview,
            commands::rbxml_import_start,
            commands::rbxml_import_cancel,
            commands::player_devices,
            commands::player_load,
            commands::player_set_device,
            commands::player_play,
            commands::player_pause,
            commands::player_seek,
            commands::player_volume,
            commands::player_state,
            commands::overview_waveform,
            commands::settings_get,
            commands::settings_set,
            commands::library_roots,
            commands::library_root_add,
            commands::library_root_remove,
            commands::cache_dir_move,
            commands::devices_list,
            commands::device_eject,
            commands::profiles_list,
            commands::profile_identify,
            commands::profile_create,
            commands::profile_link,
            commands::profile_rename,
            commands::profile_set_target,
            commands::profile_selection,
            commands::profile_set_selection,
            commands::profile_status,
            commands::export_plan,
            commands::export_apply,
            commands::export_cancel,
            commands::convert_plan,
            commands::convert_apply,
            commands::convert_cancel,
            history::history_sessions,
            history::history_session,
            history::history_session_update,
            history::history_session_delete,
            history::history_link_entry,
            history::history_track_sessions,
            history::history_scan,
            history::history_import,
            adopt::adopt_scan,
            adopt::adopt_apply,
            analysis::analysis_subscribe,
            analysis::analysis_pause,
            analysis::analysis_resume,
            analysis::analysis_cancel,
            analysis::analysis_reanalyse,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
