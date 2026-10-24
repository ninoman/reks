/**
 * Library backups (reks-core `backup`, D36): the list in Settings → Library →
 * Backups, Back up now, and Restore (which restarts the app on the restored
 * library). `offerBackupWarning` tells the user at start-up when the last
 * backup failed.
 */
import { create } from "zustand";

import {
  type BackupInfo,
  type BackupStatus,
  backupNow,
  backupRestart,
  backupRestore,
  backupShowFolder,
  backupStatus,
  backupsList,
} from "../lib/ipc";
import { useSettings } from "./settings";
import { useToasts } from "./toasts";

export interface BackupsStore {
  /** `null` until the first load finishes. */
  backups: BackupInfo[] | null;
  status: BackupStatus | null;
  busy: "backup" | "restore" | null;
  error: string | null;

  load(): Promise<void>;
  backUpNow(): Promise<void>;
  /** Restores `name`, then restarts the app. */
  restore(name: string): Promise<void>;
  showFolder(): Promise<void>;
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export const useBackups = create<BackupsStore>()((set, get) => {
  /** Runs `action`; a failure becomes `error`, a success clears it. */
  async function run(action: () => Promise<void>) {
    try {
      await action();
      set({ error: null });
    } catch (e) {
      set({ error: message(e) });
    }
  }
  const reload = async () => {
    const [backups, status] = await Promise.all([backupsList(), backupStatus()]);
    set({ backups, status });
  };

  return {
    backups: null,
    status: null,
    busy: null,
    error: null,

    load: () => run(reload),

    async backUpNow() {
      if (get().busy !== null) return;
      set({ busy: "backup" });
      // A failed backup is in the refreshed status as well as in `error`.
      await run(async () => {
        try {
          await backupNow();
        } finally {
          await reload();
        }
      });
      set({ busy: null });
    },

    async restore(name) {
      if (get().busy !== null) return;
      set({ busy: "restore" });
      try {
        await backupRestore(name);
      } catch (e) {
        set({ error: message(e), busy: null });
        return;
      }
      // The app restarts; `busy` stays on until it does.
      await run(backupRestart);
      if (get().error !== null) set({ busy: null });
    },

    showFolder: () => run(backupShowFolder),
  };
});

/** Toasts once at start-up when the library's last backup failed. */
export async function offerBackupWarning(): Promise<void> {
  let status: BackupStatus;
  try {
    status = await backupStatus();
  } catch {
    return;
  }
  if (status.lastFailure === null) return;
  useToasts.getState().show("The last library backup failed. Your library is not protected.", {
    label: "Details",
    run: () => useSettings.getState().setDialogOpen(true, "library"),
  });
}
