/** Words for library backups (Settings → Library → Backups). Presentation only. */
import type { BackupInfo, BackupReason } from "./ipc";

const REASONS: Record<BackupReason, string> = {
  startup: "Automatic, at start-up",
  migration: "Before a library update",
  manual: "Backed up by you",
  beforeRestore: "Before a restore",
  xmlImport: "Before a rekordbox import",
  convertReplace: "Before converting files",
  historyImport: "Before a history import",
};

export function reasonLabel(reason: BackupReason): string {
  return REASONS[reason];
}

/** Why a snapshot cannot be restored, or `null` if it can. */
export function restoreBlock(b: BackupInfo): string | null {
  if (b.problem !== null) return "This backup is damaged.";
  if (b.tooNew) return "Made by a newer reks. Update reks to restore it.";
  return null;
}
