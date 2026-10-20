// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { SessionDetail, SessionEntryRow } from "../lib/ipc";

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
const { useHistory } = await import("../store/history");
const { SessionView } = await import("./SessionView");

function entry(position: number, patch: Partial<SessionEntryRow>): SessionEntryRow {
  return {
    position,
    kind: "track",
    trackId: position + 10,
    title: `Track ${position}`,
    artist: "Ana",
    durationMs: 300_000,
    bpmX100: 12_400,
    key: null,
    rawTitle: `Raw ${position}`,
    rawArtist: "Raw Artist",
    rawPath: `/Contents/x/${position}.mp3`,
    rawDurationMs: 300_000,
    ...patch,
  };
}

const session: SessionDetail = {
  session: {
    id: 2,
    name: "HISTORY 002",
    sourceName: "HISTORY 002",
    playedOn: "2026-06-20",
    venue: null,
    excluded: false,
    excludedReason: null,
    tracks: 3,
    unmatched: 1,
    importedAt: "2026-10-01T12:00:00Z",
  },
  notes: null,
  volumeSerial: "1234-ABCD",
  entries: [
    entry(0, {}),
    entry(1, { kind: "divider", trackId: null, title: null, rawTitle: "---- PEAK ----" }),
    entry(2, { trackId: null, title: null, artist: null, rawTitle: "Untitled Edit" }),
    entry(3, {}),
  ],
};

beforeEach(() => {
  vi.mocked(ipc.historySessions).mockResolvedValue([session.session]);
  vi.mocked(ipc.historySession).mockResolvedValue(session);
  vi.mocked(ipc.historySessionUpdate).mockResolvedValue();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  useHistory.setState({
    sessions: [session.session],
    openId: 2,
    detail: session,
    showDividers: false,
    confirmDelete: null,
    locating: null,
    error: null,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const rows = () =>
  within(screen.getByRole("table", { name: "Session entries" }))
    .getAllByRole("row")
    .slice(1);

describe("SessionView", () => {
  it("lists entries in play order, dividers hidden, unmatched greyed with Locate…", () => {
    render(<SessionView />);
    expect(rows().map((r) => r.textContent?.slice(0, 14))).toEqual([
      "1Track 0Ana124",
      "2Untitled Edit",
      "3Track 3Ana124",
    ]);
    const unmatched = rows()[1] as HTMLElement;
    expect(unmatched.hasAttribute("data-unmatched")).toBe(true);
    expect(within(unmatched).getByRole("button", { name: "Locate…" })).toBeTruthy();
    expect(within(rows()[0] as HTMLElement).queryByRole("button")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Show 1 divider" }));
    expect(rows()[1]?.textContent).toContain("---- PEAK ----");
  });

  it("Practice toggles, rename saves on Enter", async () => {
    render(<SessionView />);
    fireEvent.click(screen.getByRole("switch"));
    await waitFor(() =>
      expect(ipc.historySessionUpdate).toHaveBeenCalledWith(2, { excluded: true }),
    );
    const name = screen.getByLabelText("Session name");
    fireEvent.change(name, { target: { value: "Panorama Bar" } });
    fireEvent.blur(name);
    await waitFor(() =>
      expect(ipc.historySessionUpdate).toHaveBeenCalledWith(2, { name: "Panorama Bar" }),
    );
  });

  it("Locate… links the entry to the picked track", async () => {
    vi.mocked(ipc.libraryQuery).mockResolvedValue({
      rows: [{ id: 99, title: "Untitled Edit", artist: "Ana", durationMs: 1000 } as never],
      total: 1,
      dividers: [],
      dividerView: false,
    });
    render(<SessionView />);
    fireEvent.click(screen.getByRole("button", { name: "Locate…" }));
    const option = await screen.findByRole("option", { name: /Untitled Edit/ });
    expect(vi.mocked(ipc.libraryQuery).mock.lastCall?.[0].text).toBe("Raw Artist Untitled Edit");
    fireEvent.click(option);
    fireEvent.click(screen.getByRole("button", { name: "Link" }));
    await waitFor(() => expect(ipc.historyLinkEntry).toHaveBeenCalledWith(2, 2, 99));
  });

  it("delete confirms first", async () => {
    render(<SessionView />);
    fireEvent.click(screen.getByRole("button", { name: "Delete…" }));
    expect(screen.getByText("Delete session?")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(ipc.historySessionDelete).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete…" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(ipc.historySessionDelete).toHaveBeenCalledWith(2));
  });
});
