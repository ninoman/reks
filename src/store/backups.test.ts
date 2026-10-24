import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BackupInfo, BackupStatus } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  backupsList: vi.fn(),
  backupStatus: vi.fn(),
  backupNow: vi.fn(),
  backupRestore: vi.fn(),
  backupRestart: vi.fn(),
  backupShowFolder: vi.fn(),
}));
vi.mock("./settings", () => ({
  useSettings: { getState: () => ({ setDialogOpen: vi.fn() }) },
}));

const ipc = await import("../lib/ipc");
const { offerBackupWarning, useBackups } = await import("./backups");
const { useToasts } = await import("./toasts");

const backup: BackupInfo = {
  name: "reks-20261024T153012123Z-manual.sqlite",
  takenAt: "2026-10-24T15:30:12.123Z",
  reason: "manual",
  sizeBytes: 2048,
  contents: { schemaVersion: 12, tracks: 3, playlists: 1 },
  problem: null,
  tooNew: false,
};
const ok: BackupStatus = { dir: "/data/library-backups", lastFailure: null };

beforeEach(() => {
  vi.resetAllMocks();
  useBackups.setState({ backups: null, status: null, busy: null, error: null });
  useToasts.setState({ toast: null });
  vi.mocked(ipc.backupsList).mockResolvedValue([backup]);
  vi.mocked(ipc.backupStatus).mockResolvedValue(ok);
});

describe("backups store", () => {
  it("loads the list and the status", async () => {
    await useBackups.getState().load();
    expect(useBackups.getState().backups).toEqual([backup]);
    expect(useBackups.getState().status).toEqual(ok);
  });

  it("keeps the error and an empty list when loading fails", async () => {
    vi.mocked(ipc.backupsList).mockRejectedValue(new Error("cannot read the folder"));
    await useBackups.getState().load();
    expect(useBackups.getState().error).toBe("cannot read the folder");
    expect(useBackups.getState().backups).toBeNull();
  });

  it("backs up and refreshes, also when the backup fails", async () => {
    vi.mocked(ipc.backupNow).mockResolvedValue(backup);
    await useBackups.getState().backUpNow();
    expect(useBackups.getState().backups).toEqual([backup]);
    expect(useBackups.getState().busy).toBeNull();

    const failed: BackupStatus = {
      dir: ok.dir,
      lastFailure: { reason: "manual", at: "2026-10-24T16:00:00.000Z", message: "disk full" },
    };
    vi.mocked(ipc.backupNow).mockRejectedValue(new Error("disk full"));
    vi.mocked(ipc.backupStatus).mockResolvedValue(failed);
    await useBackups.getState().backUpNow();
    expect(useBackups.getState().error).toBe("disk full");
    expect(useBackups.getState().status).toEqual(failed);
    expect(useBackups.getState().busy).toBeNull();
  });

  it("restores, then restarts", async () => {
    vi.mocked(ipc.backupRestore).mockResolvedValue();
    vi.mocked(ipc.backupRestart).mockResolvedValue();
    await useBackups.getState().restore(backup.name);
    expect(ipc.backupRestore).toHaveBeenCalledWith(backup.name);
    expect(ipc.backupRestart).toHaveBeenCalled();
  });

  it("does not restart when the restore is refused", async () => {
    vi.mocked(ipc.backupRestore).mockRejectedValue(new Error("newer schema"));
    await useBackups.getState().restore(backup.name);
    expect(ipc.backupRestart).not.toHaveBeenCalled();
    expect(useBackups.getState().error).toBe("newer schema");
    expect(useBackups.getState().busy).toBeNull();
  });
});

describe("offerBackupWarning", () => {
  it("is quiet when the last backup worked", async () => {
    await offerBackupWarning();
    expect(useToasts.getState().toast).toBeNull();
  });

  it("warns when it failed", async () => {
    vi.mocked(ipc.backupStatus).mockResolvedValue({
      dir: ok.dir,
      lastFailure: { reason: "startup", at: "2026-10-24T16:00:00.000Z", message: "disk full" },
    });
    await offerBackupWarning();
    expect(useToasts.getState().toast?.message).toMatch(/backup failed/);
  });
});
