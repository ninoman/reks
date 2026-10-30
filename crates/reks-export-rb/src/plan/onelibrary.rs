//! The planner's OneLibrary step (Phase 4 task 8, D14), after [`super::plan`]
//! and [`super::guard_history`]. Every export runs it (`apply` refuses a
//! plan that skipped it):
//!
//! - With [`ExportOptions::onelibrary`] on (the default) and a key, the plan
//!   warns that the export is experimental and lists what the
//!   [`OneLibraryExport`] writes and removes ([`ExportPlan::onelibrary`]).
//! - Otherwise (option off, or no key in a build from source:
//!   [`Warning::OneLibraryNoKey`]), baken removes the stick's OneLibrary
//!   files, so they are listed in [`ExportPlan::deletes`]
//!   ([`OpReason::OneLibraryRemoved`]). When `exportLibrary.db` is among
//!   them, [`Warning::OneLibraryRemoved`] names the players that then find no
//!   library, and the plan blocks until
//!   [`ExportOptions::onelibrary_removal_confirmed`], as discarding history
//!   does (review F02).
//!
//! History in the stick's existing `exportLibrary.db` (Phase 4 task 11)
//! goes through the export guard like `export.pdb`'s whenever a key opens
//! it, whether or not this export writes a new one (review F08): the
//! sessions read there ([`ExistingLibrary::Read`]) are scanned against the
//! library ([`ExportPlan::onelibrary_history_import`]); those not imported
//! fill [`HistoryGuard::onelibrary_sessions`](super::HistoryGuard::onelibrary_sessions)
//! and block with [`Problem::OneLibraryHistoryNotImported`] until
//! [`ExportOptions::history_chosen`]. `Import` imports them with the
//! `export.pdb` sessions, then backs the file up; `BackUp` backs it up;
//! `Discard` needs its confirmation. The backup runs before anything on the
//! stick changes (`apply`). A database not read keeps the Phase 2
//! safeguard only, as an unreadable `export.pdb` does.
//!
//! [`ExportOptions::onelibrary`]: super::ExportOptions::onelibrary
//! [`ExportOptions::onelibrary_removal_confirmed`]: super::ExportOptions::onelibrary_removal_confirmed
//! [`ExportOptions::history_chosen`]: super::ExportOptions::history_chosen

use reks_core::history::{scan, SourceHistory};
use reks_core::{Library, OneLibraryKey};

use super::guard::unimported;
use super::{to_i64, ExportPlan, OpReason, PlanError, Problem, Warning};
use crate::onelibrary::{library_files, ExistingLibrary, OneLibraryExport, DB_PATH};

/// Fills [`ExportPlan::onelibrary`] or the OneLibrary deletes, and adds the
/// step's warnings and problems (module docs). `key` is
/// [`crate::onelibrary::resolve_key`]'s; `lib` is scanned for the
/// database's history. Running it again replaces what an earlier run added.
/// Reads the stick and the library only.
pub fn plan_onelibrary(
    plan: &mut ExportPlan,
    exporter: &dyn OneLibraryExport,
    key: Option<&OneLibraryKey>,
    lib: &Library,
) -> Result<(), PlanError> {
    clear(plan);
    plan.onelibrary_planned = true;
    if plan.options.onelibrary {
        plan.warnings.push(Warning::OneLibraryExperimental);
        if key.is_none() {
            plan.warnings.push(Warning::OneLibraryNoKey);
        }
    }

    let existing = match key {
        Some(key) if plan.options.onelibrary => {
            let ol = exporter.plan(plan, key)?;
            plan.warnings.extend(ol.warnings.iter().cloned());
            plan.errors.extend(ol.errors.iter().cloned());
            let existing = ol.existing.clone();
            plan.onelibrary = Some(ol);
            existing
        }
        Some(key) if plan.history.onelibrary_db => {
            exporter.existing(&plan.device.mount_point, key)?
        }
        _ => ExistingLibrary::Absent,
    };
    if plan.onelibrary.is_none() {
        plan_removal(plan, key.is_none())?;
    }

    if let ExistingLibrary::Read { history, .. } = &existing {
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
    Ok(())
}

/// This export writes no `exportLibrary.db`: the stick's OneLibrary files go
/// (module docs).
fn plan_removal(plan: &mut ExportPlan, no_key: bool) -> Result<(), PlanError> {
    let files = library_files(&plan.device.mount_point, OpReason::OneLibraryRemoved)?;
    if files.is_empty() {
        return Ok(());
    }
    let freed: u64 = files.iter().map(|f| f.bytes).sum();
    plan.bytes_free_after = plan.bytes_free_after.saturating_add(to_i64(freed));
    if files.iter().any(|f| f.dst_rel == DB_PATH) {
        plan.warnings.push(Warning::OneLibraryRemoved {
            files: files.iter().map(|f| f.dst_rel.clone()).collect(),
            no_key: no_key && plan.options.onelibrary,
        });
        if !plan.options.onelibrary_removal_confirmed {
            plan.errors.push(Problem::OneLibraryRemovalNotConfirmed);
        }
    }
    plan.deletes.extend(files);
    Ok(())
}

/// Takes out what an earlier [`plan_onelibrary`] added.
fn clear(plan: &mut ExportPlan) {
    let previous = plan.onelibrary.take();
    plan.guard.onelibrary_sessions.clear();
    plan.onelibrary_history_import = None;
    let freed: u64 = plan
        .deletes
        .iter()
        .filter(|f| f.reason == OpReason::OneLibraryRemoved)
        .map(|f| f.bytes)
        .sum();
    plan.bytes_free_after = plan.bytes_free_after.saturating_sub(to_i64(freed));
    plan.deletes
        .retain(|f| f.reason != OpReason::OneLibraryRemoved);
    plan.warnings.retain(|w| {
        !matches!(
            w,
            Warning::OneLibraryExperimental
                | Warning::OneLibraryNoKey
                | Warning::OneLibraryRemoved { .. }
        ) && !previous.as_ref().is_some_and(|p| p.warnings.contains(w))
    });
    plan.errors.retain(|e| {
        !matches!(
            e,
            Problem::OneLibraryHistoryNotImported { .. } | Problem::OneLibraryRemovalNotConfirmed
        ) && !previous.as_ref().is_some_and(|p| p.errors.contains(e))
    });
}

#[cfg(test)]
mod tests;
