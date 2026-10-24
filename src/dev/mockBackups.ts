/**
 * In-memory library backups for the dev mock backend: a few snapshots, Back up
 * now adds one, Restore always succeeds (the real app then restarts).
 */
import type { BackupInfo, BackupStatus } from "../lib/ipc";

const HOUR = 3_600_000;

export function createMockBackups() {
  const make = (ago: number, reason: BackupInfo["reason"], tracks: number): BackupInfo => {
    const takenAt = new Date(Date.now() - ago).toISOString();
    return {
      name: `reks-${takenAt.replace(/[-:.]/g, "")}-${reason}.sqlite`,
      takenAt,
      reason,
      sizeBytes: 18_400_000 + tracks * 900,
      contents: { schemaVersion: 12, tracks, playlists: 41 },
      problem: null,
      tooNew: false,
    };
  };
  const list: BackupInfo[] = [
    make(2 * HOUR, "startup", 3012),
    make(26 * HOUR, "startup", 3010),
    make(50 * HOUR, "xmlImport", 2984),
  ];
  const status: BackupStatus = {
    dir: "/Users/dj/Library/Application Support/reks/library-backups",
    lastFailure: null,
  };

  return (cmd: string, args: Record<string, unknown>): unknown => {
    switch (cmd) {
      case "backups_list":
        return list;
      case "backup_status":
        return status;
      case "backup_now": {
        const b = make(0, "manual", 3012);
        list.unshift(b);
        return b;
      }
      case "backup_restore":
        if (!list.some((b) => b.name === args.name)) throw new Error("backup not found");
        return null;
      case "backup_restart":
      case "backup_show_folder":
        return null;
      default:
        return undefined;
    }
  };
}
