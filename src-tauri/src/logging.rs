//! Log file and crash files (D34). Logs go to a daily rolling file in the
//! app log dir; a panic writes `crash-<utc>.txt` there first, because
//! release builds abort on panic and the non-blocking log writer may not
//! flush. Nothing here leaves the machine; reports read from these files
//! only when the user sends one.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use reks_core::diagnostics::LOG_TAIL_BYTES;
use reks_core::timefmt::now_utc;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

const LOG_PREFIX: &str = "reks";
const LOG_SUFFIX: &str = "log";
const KEEP_LOGS: usize = 5;
const CRASH_PREFIX: &str = "crash-";
const KEEP_CRASHES: usize = 5;
/// Holds the name of the newest crash the user was already told about.
const SEEN_MARKER: &str = "crash-seen";
/// Older crashes are not attached to reports.
const CRASH_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// The app log dir, as managed state.
#[derive(Debug)]
pub struct LogDir(pub PathBuf);

/// Keeps the log writer thread alive for the life of the process.
static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Starts logging to `dir` (and to stderr in dev builds) and installs the
/// panic hook.
pub fn init(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(dir)?;
    install_panic_hook(dir.to_path_buf());
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix(LOG_SUFFIX)
        .max_log_files(KEEP_LOGS)
        .build(dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = GUARD.set(guard);
    let builder = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO);
    #[cfg(debug_assertions)]
    let builder = {
        use tracing_subscriber::fmt::writer::MakeWriterExt;
        builder.with_writer(writer.and(std::io::stderr))
    };
    #[cfg(not(debug_assertions))]
    let builder = builder.with_writer(writer);
    builder.try_init().map_err(|e| e.to_string())?;
    tracing::info!(
        version = reks_core::VERSION,
        os = std::env::consts::OS,
        "reks started"
    );
    Ok(())
}

fn install_panic_hook(dir: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let now = now_utc();
        let text = format!(
            "reks {} on {} ({})\n{now}\n\n{info}\n\n{}",
            reks_core::VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::backtrace::Backtrace::force_capture(),
        );
        let name = format!("{CRASH_PREFIX}{}.txt", now.replace(':', "-"));
        // Best effort: a panic hook has nowhere to report its own failure.
        let _ = fs::write(dir.join(name), text);
        default(info);
    }));
}

/// File names in `dir` that start with `prefix` and end with `suffix`,
/// oldest first (their names carry a sortable UTC date).
fn named(dir: &Path, prefix: &str, suffix: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(suffix))
        .collect();
    names.sort();
    names
}

fn crashes(dir: &Path) -> Vec<String> {
    named(dir, CRASH_PREFIX, ".txt")
}

/// The newest crash report younger than a week.
pub fn recent_crash(dir: &Path) -> Option<String> {
    let name = crashes(dir).pop()?;
    let path = dir.join(name);
    let age = fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())?;
    (age <= CRASH_MAX_AGE).then(|| fs::read_to_string(&path).ok())?
}

/// Whether a crash happened since the user was last told about one. Marks
/// it as told and prunes old crash files.
pub fn take_new_crash(dir: &Path) -> bool {
    let mut names = crashes(dir);
    let Some(newest) = names.last().cloned() else {
        return false;
    };
    let marker = dir.join(SEEN_MARKER);
    let seen = fs::read_to_string(&marker).unwrap_or_default();
    let is_new = newest.as_str() > seen.trim();
    if is_new {
        if let Err(err) = fs::write(&marker, &newest) {
            tracing::warn!(%err, "could not mark the crash as seen");
        }
    }
    let excess = names.len().saturating_sub(KEEP_CRASHES);
    for old in names.drain(..excess) {
        let _ = fs::remove_file(dir.join(old));
    }
    is_new
}

/// The end of the newest log file, at most a couple of tail budgets long
/// (the report trims it further on a line boundary).
pub fn log_tail(dir: &Path) -> String {
    let Some(name) = named(dir, LOG_PREFIX, LOG_SUFFIX).pop() else {
        return String::new();
    };
    read_end(&dir.join(name), 2 * LOG_TAIL_BYTES as u64).unwrap_or_default()
}

fn read_end(path: &Path, max: u64) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(max)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_crash_is_reported_once_and_old_ones_pruned() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!take_new_crash(dir.path()));
        for day in 1..=7 {
            fs::write(
                dir.path()
                    .join(format!("crash-2026-10-0{day}T00-00-00.000Z.txt")),
                "boom",
            )
            .unwrap();
        }
        assert!(take_new_crash(dir.path()));
        assert!(!take_new_crash(dir.path()));
        assert_eq!(crashes(dir.path()).len(), KEEP_CRASHES);
        assert_eq!(recent_crash(dir.path()).as_deref(), Some("boom"));
        fs::write(
            dir.path().join("crash-2026-10-08T00-00-00.000Z.txt"),
            "again",
        )
        .unwrap();
        assert!(take_new_crash(dir.path()));
    }

    #[test]
    fn log_tail_reads_the_newest_file_end() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(log_tail(dir.path()), "");
        fs::write(dir.path().join("reks.2026-10-01.log"), "old").unwrap();
        let long = format!("{}end", "x".repeat(3 * LOG_TAIL_BYTES));
        fs::write(dir.path().join("reks.2026-10-02.log"), &long).unwrap();
        let tail = log_tail(dir.path());
        assert!(tail.ends_with("end"));
        assert_eq!(tail.len(), 2 * LOG_TAIL_BYTES);
    }
}
