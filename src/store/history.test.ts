import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SessionDetail, SessionRow } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  historySessions: vi.fn(),
  historySession: vi.fn(),
  historySessionUpdate: vi.fn(),
  historySessionDelete: vi.fn(),
  historyLinkEntry: vi.fn(),
  libraryQuery: vi.fn(),
  libraryFacets: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { groupSessions, useHistory } = await import("./history");
const { useLibrary } = await import("./library");

function row(id: number, playedOn: string | null, patch: Partial<SessionRow> = {}): SessionRow {
  return {
    id,
    name: `HISTORY ${id}`,
    sourceName: `HISTORY ${id}`,
    playedOn,
    venue: null,
    excluded: false,
    excludedReason: null,
    tracks: 3,
    unmatched: 0,
    importedAt: "2026-10-01T12:00:00Z",
    ...patch,
  };
}

const detail = (s: SessionRow): SessionDetail => ({
  session: s,
  notes: null,
  volumeSerial: null,
  entries: [],
});

describe("groupSessions", () => {
  it("groups year → month → session in list order, undated last", () => {
    const groups = groupSessions([
      row(4, "2026-09-05"),
      row(3, "2026-06-27"),
      row(2, "2026-06-20"),
      row(1, "2025-12-31"),
      row(5, null),
    ]);
    expect(groups.map((g) => g.label)).toEqual(["2026", "2025", "Date unknown"]);
    expect(groups[0]?.months.map((m) => [m.label, m.sessions.map((s) => s.id)])).toEqual([
      ["September", [4]],
      ["June", [3, 2]],
    ]);
    expect(groups[2]?.months).toEqual([]);
    expect(groups[2]?.sessions.map((s) => s.id)).toEqual([5]);
  });
});

describe("useHistory", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useHistory.setState({ sessions: [], expanded: new Set(), openId: null, detail: null });
    vi.mocked(ipc.libraryQuery).mockResolvedValue({
      rows: [],
      total: 0,
      dividers: [],
      dividerView: false,
    });
  });

  it("first load opens the newest year and month", async () => {
    vi.mocked(ipc.historySessions).mockResolvedValue([row(2, "2026-06-20"), row(1, "2025-01-01")]);
    await useHistory.getState().load();
    expect([...useHistory.getState().expanded]).toEqual(["2026", "2026-06"]);
  });

  it("Practice reloads the session, bumps the version and refreshes the table", async () => {
    const s = row(2, "2026-06-20");
    vi.mocked(ipc.historySessions).mockResolvedValue([s]);
    vi.mocked(ipc.historySession).mockResolvedValue(detail(s));
    await useHistory.getState().open(2);
    const version = useHistory.getState().version;
    const generation = useLibrary.getState().generation;

    vi.mocked(ipc.historySession).mockResolvedValue(detail({ ...s, excluded: true }));
    await useHistory.getState().update({ excluded: true });
    expect(ipc.historySessionUpdate).toHaveBeenCalledWith(2, { excluded: true });
    expect(useHistory.getState().detail?.session.excluded).toBe(true);
    expect(useHistory.getState().version).toBe(version + 1);
    expect(useLibrary.getState().generation).toBeGreaterThan(generation);
  });

  it("a rename doesn't touch counts; a failed edit shows the error", async () => {
    const s = row(2, "2026-06-20");
    vi.mocked(ipc.historySessions).mockResolvedValue([s]);
    vi.mocked(ipc.historySession).mockResolvedValue(detail(s));
    await useHistory.getState().open(2);
    const version = useHistory.getState().version;
    await useHistory.getState().update({ name: "Garage" });
    expect(useHistory.getState().version).toBe(version);

    vi.mocked(ipc.historySessionUpdate).mockRejectedValue("invalid name: must not be empty");
    await useHistory.getState().update({ name: " " });
    expect(useHistory.getState().error).toContain("must not be empty");
  });

  it("delete asks first and closes the session", async () => {
    const s = row(2, "2026-06-20");
    vi.mocked(ipc.historySessions).mockResolvedValue([s]);
    vi.mocked(ipc.historySession).mockResolvedValue(detail(s));
    await useHistory.getState().load();
    await useHistory.getState().open(2);
    useHistory.getState().requestDelete();
    expect(useHistory.getState().confirmDelete?.id).toBe(2);
    await useHistory.getState().resolveDelete(false);
    expect(ipc.historySessionDelete).not.toHaveBeenCalled();

    useHistory.getState().requestDelete();
    vi.mocked(ipc.historySessions).mockResolvedValue([]);
    await useHistory.getState().resolveDelete(true);
    expect(ipc.historySessionDelete).toHaveBeenCalledWith(2);
    expect(useHistory.getState().openId).toBeNull();
    expect(useHistory.getState().sessions).toEqual([]);
  });
});
