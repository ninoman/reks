//! The planner's OneLibrary step (Phase 4 task 8, D14), after [`super::plan`]
//! and [`super::guard_history`]: with [`ExportOptions::onelibrary`] on, the
//! plan warns that the export is experimental, needs a key, and lists what
//! the [`OneLibraryExport`] would write and remove.
//!
//! History in the stick's existing `exportLibrary.db` reuses the export
//! guard: sessions the writer read there ([`ExistingLibrary::Read`]) count in
//! [`HistoryGuard::onelibrary_sessions`](super::HistoryGuard::onelibrary_sessions)
//! and block with [`Problem::OneLibraryHistoryNotImported`] until
//! [`ExportOptions::history_chosen`]. Every choice keeps them safe or
//! discards them on purpose: `Import` and `BackUp` back the file up (Phase 2
//! safeguard), `Discard` needs its confirmation. A database not read keeps
//! the Phase 2 safeguard only, as an unreadable `export.pdb` does.
//!
//! [`ExportOptions::onelibrary`]: super::ExportOptions::onelibrary
//! [`ExportOptions::history_chosen`]: super::ExportOptions::history_chosen

use reks_core::OneLibraryKey;

use super::{ExportPlan, PlanError, Problem, Warning};
use crate::onelibrary::{ExistingLibrary, OneLibraryExport};

/// Fills [`ExportPlan::onelibrary`] and adds the step's warnings and
/// problems (module docs). `key` is [`crate::onelibrary::resolve_key`]'s.
/// Running it again replaces what an earlier run added. Reads the stick only.
pub fn plan_onelibrary(
    plan: &mut ExportPlan,
    exporter: &dyn OneLibraryExport,
    key: Option<&OneLibraryKey>,
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
    if let ExistingLibrary::Read { sessions, .. } = ol.existing {
        if sessions > 0 {
            plan.guard.onelibrary_sessions = sessions;
            if !plan.options.history_chosen {
                plan.errors
                    .push(Problem::OneLibraryHistoryNotImported { sessions });
            }
        }
    }
    plan.onelibrary = Some(ol);
    Ok(())
}

/// Takes out what an earlier [`plan_onelibrary`] added.
fn clear(plan: &mut ExportPlan) {
    let previous = plan.onelibrary.take();
    plan.guard.onelibrary_sessions = 0;
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
