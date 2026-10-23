//! Bug report and feedback commands (D33). Gathers diagnostics, renders the
//! report with `reks_core::diagnostics`, and posts it to the feedback relay
//! only when the user presses Send.

use std::path::PathBuf;
use std::time::Duration;

use reks_core::diagnostics::{self, Diagnostics, Redactions, Report, ReportDraft};
use reks_core::{Library, TrackQuery};
use tauri::{AppHandle, Manager, State};

use crate::commands::{blocking, CmdError, CmdResult};
use crate::logging::{self, LogDir};
use crate::state::AppState;

/// Relay endpoint, set at build time (docs/feedback.md). Unset: reports can
/// only be saved to a file.
const RELAY_URL: Option<&str> = option_env!("REKS_FEEDBACK_URL");
const SEND_TIMEOUT: Duration = Duration::from_secs(20);

/// Renders `draft` exactly as it would be sent.
async fn render(
    app: &AppHandle,
    state: &State<'_, AppState>,
    draft: ReportDraft,
) -> CmdResult<Report> {
    let log_dir = app.state::<LogDir>().0.clone();
    let home = app.path().home_dir().ok();
    blocking(state, move |lib| -> reks_core::Result<Report> {
        let roots: Vec<String> = lib.library_roots()?.into_iter().map(|r| r.path).collect();
        let redactions = Redactions::new(home.as_deref(), &roots);
        let diag = if draft.include_diagnostics {
            Some(gather(lib, &log_dir)?)
        } else {
            None
        };
        Ok(diagnostics::render(&draft, diag.as_ref(), &redactions))
    })
    .await
}

fn gather(lib: &Library, log_dir: &std::path::Path) -> reks_core::Result<Diagnostics> {
    let tracks = lib.tracks(&TrackQuery {
        limit: 1,
        ..TrackQuery::default()
    })?;
    let playlists = lib
        .playlists()?
        .nodes
        .iter()
        .filter(|n| !n.is_folder)
        .count();
    Ok(Diagnostics {
        app_version: reks_core::VERSION.to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        schema_version: reks_core::db::SCHEMA_VERSION,
        track_count: tracks.total,
        playlist_count: u64::try_from(playlists).unwrap_or(u64::MAX),
        crash: logging::recent_crash(log_dir),
        log: logging::log_tail(log_dir),
    })
}

/// Whether this build can send reports (it knows a relay).
#[tauri::command]
pub fn report_can_send() -> bool {
    RELAY_URL.is_some()
}

/// Whether the app crashed since the user was last told; tells them once.
#[tauri::command]
pub async fn report_new_crash(log_dir: State<'_, LogDir>) -> CmdResult<bool> {
    let dir = log_dir.0.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || logging::take_new_crash(&dir)).await?)
}

/// The report as it would be sent, for the "what's included" preview.
#[tauri::command]
pub async fn report_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ReportDraft,
) -> CmdResult<Report> {
    render(&app, &state, draft).await
}

/// Sends the report to the relay, which puts it on the triage board.
#[tauri::command]
pub async fn report_send(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ReportDraft,
) -> CmdResult<()> {
    diagnostics::validate(&draft)?;
    let url = RELAY_URL.ok_or_else(|| CmdError::new("this build cannot send reports".into()))?;
    let report = render(&app, &state, draft).await?;
    let response = reqwest::Client::builder()
        .timeout(SEND_TIMEOUT)
        .build()?
        .post(url)
        .header("X-Reks-Version", reks_core::VERSION)
        .json(&report)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let reason = response.text().await.unwrap_or_default();
        tracing::warn!(%status, %reason, "report relay refused the report");
        return Err(CmdError::new(format!(
            "the report server answered {status}: {reason}"
        )));
    }
    tracing::info!("report sent");
    Ok(())
}

/// Writes the report to `path`, for sending by other means.
#[tauri::command]
pub async fn report_save(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ReportDraft,
    path: PathBuf,
) -> CmdResult<()> {
    let report = render(&app, &state, draft).await?;
    let text = format!(
        "# {}\n\nType: {:?}\n\n{}",
        report.title, report.kind, report.body
    );
    tauri::async_runtime::spawn_blocking(move || std::fs::write(path, text)).await??;
    Ok(())
}

/// Records a UI error in the log so reports include it.
#[tauri::command]
pub fn log_ui_error(message: String, stack: Option<String>) {
    tracing::error!(target: "ui", %message, stack = stack.as_deref().unwrap_or(""), "ui error");
}
