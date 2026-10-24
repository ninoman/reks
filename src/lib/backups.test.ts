import { describe, expect, it } from "vitest";

import { reasonLabel, restoreBlock } from "./backups";
import type { BackupInfo } from "./ipc";

const info = (over: Partial<BackupInfo> = {}): BackupInfo => ({
  name: "reks-20261024T153012123Z-manual.sqlite",
  takenAt: "2026-10-24T15:30:12.123Z",
  reason: "manual",
  sizeBytes: 1000,
  contents: { schemaVersion: 12, tracks: 3, playlists: 1 },
  problem: null,
  tooNew: false,
  ...over,
});

describe("restoreBlock", () => {
  it("lets a good backup restore", () => {
    expect(restoreBlock(info())).toBeNull();
  });

  it("blocks a damaged one and one from a newer reks", () => {
    expect(restoreBlock(info({ problem: "not a database", contents: null }))).toMatch(/damaged/);
    expect(restoreBlock(info({ tooNew: true }))).toMatch(/newer reks/);
  });
});

describe("reasonLabel", () => {
  it("has a plain label for every reason", () => {
    expect(reasonLabel("beforeRestore")).toBe("Before a restore");
  });
});
