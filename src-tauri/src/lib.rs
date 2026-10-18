//! Tauri shell for reks. Commands, state and events only; business logic
//! lives in `crates/*` (docs/decisions.md D2).

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
        .invoke_handler(tauri::generate_handler![core_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
