//! Export guard (Phase 2.5 task 8, D23): an export onto a stick that holds
//! play history sessions the library has not imported stops until the user
//! chooses what happens to them.
//!
//! [`guard_history`] reads the stick's history ([`read_play_history`]) and
//! runs the read-only history scan ([`reks_core::history::scan`]) against the
//! library. Sessions the scan would create or extend are
//! [`HistoryGuard::sessions`]. While there are any and
//! [`ExportOptions::history_chosen`](super::ExportOptions::history_chosen) is
//! false, the plan carries [`Problem::HistoryNotImported`] and `apply`
//! refuses it. The choices are the [`HistoryPolicy`] values:
//!
//! - `Import` ("Import now"): `apply` imports the scanned sessions with the
//!   import's default choices (all counted; Practice can be set later in
//!   History), then backs up as `BackUp` does.
//! - `BackUp` ("Back up and continue"): the Phase 2 backup only; the backup
//!   folder can be imported later (`reks history import <backup>`).
//! - `Discard` ("Continue and discard"): needs
//!   [`discard_history_confirmed`](super::ExportOptions::discard_history_confirmed)
//!   too.
//!
//! Only a stick whose `export.pdb` counts history sessions is scanned. An
//! unreadable `export.pdb` stays with the Phase 2 safeguard (backed up by
//! default). When the count succeeds but the full read fails, the guard
//! still needs a choice ([`HistoryGuard::unreadable`]) and `Import` is
//! refused ([`Problem::HistoryImportUnavailable`]).

use reks_core::history::{scan, HistoryImportPlan, SessionAction};
use reks_core::Library;
use serde::{Deserialize, Serialize};

use super::{ExportPlan, PlanError, Problem};
use crate::history::{read_play_history, HistoryPolicy, HistoryReadOptions, HistorySource};

/// A stick session the library has not imported, or holds only the start of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnimportedSession {
    /// As on the stick, e.g. `HISTORY 001`.
    pub name: String,
    /// Entries an import would add (the new tail for an extended session).
    pub entries: u32,
    /// The library holds this session's first entries; an import extends it.
    pub extends: bool,
}

/// What the guard found on the stick.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryGuard {
    /// Sessions not imported yet, in stick order.
    pub sessions: Vec<UnimportedSession>,
    /// Reading the stick's history sessions failed after `export.pdb`
    /// counted some: they may be unimported, and `Import` can't run.
    pub unreadable: Option<String>,
    /// History sessions in the stick's `exportLibrary.db`, when the
    /// OneLibrary step read it ([`super::plan_onelibrary`]). reks cannot
    /// import them yet (Phase 4 task 11); backups keep them.
    #[serde(default)]
    pub onelibrary_sessions: u32,
}

impl HistoryGuard {
    /// The export needs the user's choice before it may write.
    pub fn needs_choice(&self) -> bool {
        self.pdb_needs_choice() || self.onelibrary_sessions > 0
    }

    /// `export.pdb`'s sessions alone need a choice.
    pub(super) fn pdb_needs_choice(&self) -> bool {
        !self.sessions.is_empty() || self.unreadable.is_some()
    }

    /// Entries in [`Self::sessions`].
    pub fn entries(&self) -> u32 {
        self.sessions.iter().map(|s| s.entries).sum()
    }
}

/// Checks `plan`'s stick against `lib` (module docs): fills
/// [`ExportPlan::guard`] and [`ExportPlan::history_import`] and adds the
/// guard's problems. [`super::plan`] calls it; callers of
/// [`super::plan_sources`] must call it themselves. Reads the stick only.
pub fn guard_history(plan: &mut ExportPlan, lib: &Library) -> Result<(), PlanError> {
    // The OneLibrary step owns its count; this guard may run after it.
    plan.guard = HistoryGuard {
        onelibrary_sessions: plan.guard.onelibrary_sessions,
        ..HistoryGuard::default()
    };
    plan.history_import = None;
    plan.errors.retain(|p| {
        !matches!(
            p,
            Problem::HistoryNotImported { .. } | Problem::HistoryImportUnavailable { .. }
        )
    });
    if plan.history.pdb_sessions == 0 {
        return Ok(());
    }

    let src = HistorySource::Stick(plan.device.mount_point.clone());
    let opts = HistoryReadOptions {
        stick_serial: plan.history.volume_serial.clone(),
        // Fingerprints don't use dates, so the HIST1 heuristic stays off.
        hist1_dates: false,
    };
    match read_play_history(&src, &opts) {
        Ok(history) => {
            let scanned = scan(&history.to_source_history(), lib)?;
            plan.guard.sessions = unimported(&scanned);
            plan.history_import = Some(scanned);
        }
        Err(e) => {
            tracing::warn!(root = %plan.device.mount_point.display(), error = %e, "cannot read the stick's history sessions");
            plan.guard.unreadable = Some(e.to_string());
        }
    }

    if !plan.guard.pdb_needs_choice() {
        return Ok(());
    }
    if !plan.options.history_chosen {
        plan.errors.push(Problem::HistoryNotImported {
            sessions: if plan.guard.unreadable.is_some() {
                plan.history.pdb_sessions
            } else {
                u32::try_from(plan.guard.sessions.len()).unwrap_or(u32::MAX)
            },
        });
    } else if plan.options.history == HistoryPolicy::Import {
        if let Some(message) = &plan.guard.unreadable {
            plan.errors.push(Problem::HistoryImportUnavailable {
                message: message.clone(),
            });
        }
    }
    Ok(())
}

fn unimported(plan: &HistoryImportPlan) -> Vec<UnimportedSession> {
    plan.sessions
        .iter()
        .filter_map(|s| {
            let extends = match s.action {
                SessionAction::New => false,
                SessionAction::Extend { .. } => true,
                SessionAction::Skip(_) => return None,
            };
            Some(UnimportedSession {
                name: s.source_name.clone(),
                entries: u32::try_from(s.new_entries().len()).unwrap_or(u32::MAX),
                extends,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
