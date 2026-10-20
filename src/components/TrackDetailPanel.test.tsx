// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Track, TrackDetail } from "../lib/ipc";
import { useHistory } from "../store/history";
import { useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { emptySelection } from "../store/selection";
import { TrackDetailPanel, TrackInfoPanel } from "./TrackDetailPanel";

vi.mock("../lib/ipc", () => ({
  libraryTrack: vi.fn(),
  libraryUpdateTrack: vi.fn(),
  artworkGet: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  historyTrackSessions: vi.fn(),
  historySession: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const libraryTrack = vi.mocked(ipc.libraryTrack);
const libraryUpdateTrack = vi.mocked(ipc.libraryUpdateTrack);
const libraryFacets = vi.mocked(ipc.libraryFacets);
const historyTrackSessions = vi.mocked(ipc.historyTrackSessions);

function track(patch: Partial<Track> = {}): Track {
  return {
    id: 7,
    title: "Old Title",
    artist: "Ana",
    album: null,
    genre: "House",
    label: null,
    remixer: null,
    comment: null,
    key: "Am",
    bpmX100: 12800,
    rating: 3,
    color: "red",
    durationMs: 300_000,
    year: 2001,
    kind: "mp3",
    bitRateKbps: 320,
    dateAdded: "2026-01-01",
    playCount: 0,
    missing: false,
    needsAnalysis: false,
    entryPosition: null,
    path: "/music/old.mp3",
    fileSize: 1234,
    fileMtime: "2026-01-01T00:00:00.000Z",
    albumArtist: null,
    composer: null,
    mix: null,
    grouping: null,
    trackNo: null,
    discNo: null,
    sampleRate: 44_100,
    bitDepth: null,
    channels: 2,
    keySource: "rekordbox",
    bpmSource: "rekordbox",
    lastPlayed: null,
    rbTrackId: 1,
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z",
    artwork: null,
    ...patch,
  };
}

function detail(t: Track): TrackDetail {
  return {
    track: t,
    cues: [
      { id: 1, slot: 0, kind: 0, startMs: 120.5, endMs: null, name: "intro", colorRgb: 0xff0000 },
      { id: 2, slot: null, kind: 4, startMs: 1000, endMs: 8500, name: null, colorRgb: null },
    ],
    beatgrid: { markers: 1, firstBeatMs: 120.5, bpmMinX100: 12800, bpmMaxX100: 12800 },
  };
}

beforeEach(() => {
  libraryTrack.mockResolvedValue(detail(track()));
  historyTrackSessions.mockResolvedValue([]);
  libraryFacets.mockResolvedValue({
    genres: [],
    keys: [],
    colours: ["red", "blue"],
    playlists: [],
  });
  // The table has the track loaded on page 0.
  useLibrary.setState({ pages: new Map([[0, [track()]]]), total: 1 });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("TrackDetailPanel", () => {
  it("shows the track, its cues and grid read-only", async () => {
    render(<TrackDetailPanel trackId={7} />);
    expect(await screen.findByLabelText("Title")).toHaveProperty("value", "Old Title");
    expect(libraryTrack).toHaveBeenCalledWith(7);
    expect(screen.getByText("Hot A")).toBeTruthy();
    const positions = [...document.querySelectorAll('[data-slot="cue-pos"]')].map(
      (e) => e.textContent,
    );
    expect(positions).toEqual(["0:00.121", "0:01.000 – 0:08.500"]);
    expect(screen.getByText("128.00")).toBeTruthy();
    expect(screen.getByLabelText("No artwork")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Save" })).toHaveProperty("disabled", true);
  });

  it("edit → save sends only changed fields and updates the table row", async () => {
    const saved = track({ title: "New Title", artist: null, year: 1999 });
    libraryUpdateTrack.mockResolvedValue(detail(saved));
    render(<TrackDetailPanel trackId={7} />);

    fireEvent.change(await screen.findByLabelText("Title"), { target: { value: "New Title" } });
    fireEvent.change(screen.getByLabelText("Artist"), { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("Year"), { target: { value: "1999" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(useLibrary.getState().pages.get(0)?.[0]?.title).toBe("New Title"));
    expect(libraryUpdateTrack).toHaveBeenCalledTimes(1);
    expect(libraryUpdateTrack).toHaveBeenCalledWith(7, {
      title: "New Title",
      artist: null,
      year: 1999,
    });
    expect(useLibrary.getState().pages.get(0)?.[0]?.artist).toBeNull();
    expect(libraryFacets).toHaveBeenCalled();
    // The form now shows the saved values and is clean again.
    expect(screen.getByLabelText("Title")).toHaveProperty("value", "New Title");
    expect(screen.getByRole("button", { name: "Save" })).toHaveProperty("disabled", true);
  });

  it("a rejected save shows the error and keeps the edit", async () => {
    libraryUpdateTrack.mockRejectedValue("invalid rating: 6 is outside 0..=5");
    render(<TrackDetailPanel trackId={7} />);

    fireEvent.change(await screen.findByLabelText("Key"), { target: { value: "H#" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect((await screen.findByRole("alert")).textContent).toContain("invalid rating");
    expect(libraryUpdateTrack).toHaveBeenCalledWith(7, { key: "H#" });
    expect(screen.getByLabelText("Key")).toHaveProperty("value", "H#");
    expect(useLibrary.getState().pages.get(0)?.[0]?.title).toBe("Old Title");
  });

  it("Escape reverts unsaved edits", async () => {
    render(<TrackDetailPanel trackId={7} />);
    const title = await screen.findByLabelText("Title");
    fireEvent.change(title, { target: { value: "Draft" } });
    fireEvent.keyDown(title, { key: "Escape" });
    expect(title).toHaveProperty("value", "Old Title");
    expect(libraryUpdateTrack).not.toHaveBeenCalled();
  });
});

describe("Played in", () => {
  it("lists the sessions, practice ones marked, and opens one", async () => {
    libraryTrack.mockResolvedValue(detail(track({ playCount: 4, lastPlayed: "2026-09-05" })));
    historyTrackSessions.mockResolvedValue([
      {
        id: 4,
        name: "Garage Noord",
        playedOn: "2026-09-05",
        venue: null,
        excluded: false,
        plays: 2,
      },
      { id: 3, name: "HISTORY 003", playedOn: "2026-06-27", venue: null, excluded: true, plays: 1 },
    ]);
    vi.mocked(ipc.historySession).mockReturnValue(new Promise(() => {}));
    render(<TrackDetailPanel trackId={7} />);
    const section = await screen.findByRole("region", { name: "Played in" });
    await waitFor(() => expect(section.textContent).toContain("Garage Noord"));
    expect(historyTrackSessions).toHaveBeenCalledWith(7);
    expect(section.textContent).toContain("2026-09-05");
    expect(section.textContent).toContain("×2");
    expect(screen.getAllByText("Practice")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "HISTORY 003" }));
    expect(useHistory.getState().openId).toBe(3);
  });
});

describe("TrackInfoPanel", () => {
  it("starts closed: selecting a row doesn't open it", () => {
    expect(usePanels.getState().infoOpen).toBe(false);
  });

  it("hints when nothing is selected, shows the selected track otherwise", async () => {
    useLibrary.setState({ selection: emptySelection });
    render(<TrackInfoPanel />);
    expect(screen.getByText(/Select a track/)).toBeTruthy();
    expect(libraryTrack).not.toHaveBeenCalled();

    cleanup();
    useLibrary.setState({ selection: { selected: new Set([0]), anchor: 0, focus: 0 } });
    render(<TrackInfoPanel />);
    await waitFor(() => expect(libraryTrack).toHaveBeenCalledWith(7));
  });

  it("its close button turns the toolbar toggle off", () => {
    usePanels.setState({ infoOpen: true });
    useLibrary.setState({ selection: emptySelection });
    render(<TrackInfoPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Close track info" }));
    expect(usePanels.getState().infoOpen).toBe(false);
  });
});
