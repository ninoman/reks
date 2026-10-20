//! The planner's OneLibrary step (Phase 4 task 8, D14), after [`super::plan`]
//! and [`super::guard_history`]: with [`ExportOptions::onelibrary`] on, the
//! plan warns that the export is experimental, needs a key, and lists what
//! the [`OneLibraryExport`] would write and remove.
//!
//! History in the stick's existing `exportLibrary.db` (Phase 4 task 11)
//! goes through the export guard like `export.pdb`'s: the sessions the
//! writer read there ([`ExistingLibrary::Read`]) are scanned against the
//! library ([`ExportPlan::onelibrary_history_import`]); those not imported
//! fill [`HistoryGuard::onelibrary_sessions`](super::HistoryGuard::onelibrary_sessions)
//! and block with [`Problem::OneLibraryHistoryNotImported`] until
//! [`ExportOptions::history_chosen`]. `Import` imports them with the
//! `export.pdb` sessions, then backs the file up; `BackUp` backs it up;
//! `Discard` needs its confirmation. The backup runs before the writer
//! replaces the file (`apply`). A database not read keeps the Phase 2
//! safeguard only, as an unreadable `export.pdb` does.
//!
//! [`ExportOptions::onelibrary`]: super::ExportOptions::onelibrary
//! [`ExportOptions::history_chosen`]: super::ExportOptions::history_chosen

use reks_core::history::{scan, SourceHistory};
use reks_core::{Library, OneLibraryKey};

use super::guard::unimported;
use super::{ExportPlan, PlanError, Problem, Warning};
use crate::onelibrary::{ExistingLibrary, OneLibraryExport};

/// Fills [`ExportPlan::onelibrary`] and adds the step's warnings and
/// problems (module docs). `key` is [`crate::onelibrary::resolve_key`]'s;
/// `lib` is scanned for the database's history. Running it again replaces
/// what an earlier run added. Reads the stick and the library only.
pub fn plan_onelibrary(
    plan: &mut ExportPlan,
    exporter: &dyn OneLibraryExport,
    key: Option<&OneLibraryKey>,
    lib: &Library,
) -> Result<(), PlanError> {
    clear(plan);
    if !plan.options.onelibrary {
        return Ok(());
    }
    plan.warnings.push(Warning::OneLibraryExperimental);
    let Some(key) = key else {
        plan.errors.push(Problem::OneLibraryKeyMissing);
        return Ok(());
    };

    let ol = exporter.plan(plan, key)?;
    plan.warnings.extend(ol.warnings.iter().cloned());
    plan.errors.extend(ol.errors.iter().cloned());
    if let ExistingLibrary::Read { history, .. } = &ol.existing {
        if !history.is_empty() {
            let source = SourceHistory {
                volume_serial: plan.history.volume_serial.clone(),
                sessions: history.clone(),
            };
            let scanned = scan(&source, lib)?;
            plan.guard.onelibrary_sessions = unimported(&scanned);
            plan.onelibrary_history_import = Some(scanned);
        }
    }
    let sessions = plan.guard.onelibrary_sessions.len();
    if sessions > 0 && !plan.options.history_chosen {
        plan.errors.push(Problem::OneLibraryHistoryNotImported {
            sessions: u32::try_from(sessions).unwrap_or(u32::MAX),
        });
    }
    plan.onelibrary = Some(ol);
    Ok(())
}

/// Takes out what an earlier [`plan_onelibrary`] added.
fn clear(plan: &mut ExportPlan) {
    let previous = plan.onelibrary.take();
    plan.guard.onelibrary_sessions.clear();
    plan.onelibrary_history_import = None;
    plan.warnings.retain(|w| {
        !matches!(w, Warning::OneLibraryExperimental)
            && !previous.as_ref().is_some_and(|p| p.warnings.contains(w))
    });
    plan.errors.retain(|e| {
        !matches!(
            e,
            Problem::OneLibraryKeyMissing | Problem::OneLibraryHistoryNotImported { .. }
        ) && !previous.as_ref().is_some_and(|p| p.errors.contains(e))
    });
}

#[cfg(test)]
mod tests;
