// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { TrackRow } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { emptySelection } from "../store/selection";
import { useTags } from "../store/tags";
import { BulkEditPanel } from "./BulkEditPanel";
import { TrackInfoPanel } from "./TrackDetailPanel";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  libraryBulkUpdateTracks: vi.fn(),
  libraryTrack: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  tagsGet: vi.fn(),
  tagsMutate: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const bulk = vi.mocked(ipc.libraryBulkUpdateTracks);

function row(id: number, patch: Partial<TrackRow> = {}): TrackRow {
  return {
    id,
    title: `T${id}`,
    artist: "Ana",
    album: null,
    genre: "House",
    label: null,
    remixer: null,
    comment: null,
    key: "Am",
    bpmX100: 12800,
    rating: 3,
    color: null,
    durationMs: 1,
    year: 2001,
    kind: "mp3",
    bitRateKbps: null,
    dateAdded: "2026-01-01",
    playCount: 0,
    lastPlayed: null,
    missing: false,
    needsAnalysis: false,
    artworkId: null,
    entryPosition: null,
    tags: [],
    hotCues: [],
    ...patch,
  };
}

function select(rows: TrackRow[]) {
  useLibrary.setState({
    pages: new Map([[0, rows]]),
    total: rows.length,
    dividers: [],
    selection: {
      selected: new Set(rows.map((_, i) => i)),
      anchor: 0,
      focus: 0,
    },
    facets: { genres: [], keys: [], colours: ["red", "blue"], playlists: [] },
  });
}

beforeEach(() => {
  useTags.setState({
    status: "ready",
    tree: {
      categories: [
        {
          id: 1,
          name: "Genre",
          tags: [
            { id: 5, name: "Peak", trackCount: 2 },
            { id: 6, name: "Warm", trackCount: 0 },
          ],
        },
      ],
    },
  } as never);
  vi.mocked(ipc.libraryFacets).mockResolvedValue({
    genres: [],
    keys: [],
    colours: [],
    playlists: [],
  });
  vi.mocked(ipc.tagsGet).mockResolvedValue({ categories: [] });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useLibrary.setState({ selection: emptySelection });
});

describe("BulkEditPanel", () => {
  const rows = [row(1), row(2, { artist: "Bo", rating: 5, tags: [5] }), row(3, { tags: [5] })];

  it("shows shared values, Mixed for differing ones, and applies nothing untouched", () => {
    select(rows);
    render(<BulkEditPanel />);
    expect(screen.getByText(/3 tracks selected/)).toBeTruthy();
    expect(screen.getByLabelText("Genre")).toHaveProperty("value", "House");
    const artist = screen.getByLabelText("Artist") as HTMLInputElement;
    expect(artist.value).toBe("");
    expect(artist.placeholder).toBe("Mixed");
    expect(screen.getByLabelText("Rating")).toHaveProperty("value", "__mixed");
    expect(screen.getByRole("button", { name: "Apply" })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: "Discard" })).toHaveProperty("disabled", true);
    // Fields that are unsafe in bulk are not offered.
    expect(screen.queryByLabelText("Title")).toBeNull();
    expect(screen.queryByLabelText("Track #")).toBeNull();
  });

  it("one Apply sends only the touched fields for every track and updates the rows", async () => {
    select(rows);
    bulk.mockResolvedValue({
      rows: rows.map((r) => ({ ...r, genre: "Techno", comment: "x" })),
      tagsAdded: 0,
      tagsRemoved: 0,
    });
    render(<BulkEditPanel />);
    fireEvent.change(screen.getByLabelText("Genre"), { target: { value: "Techno" } });
    fireEvent.click(screen.getByRole("radio", { name: "Append to comment" }));
    fireEvent.change(screen.getByLabelText("Comment"), { target: { value: "x" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply to 3 tracks" }));

    await waitFor(() => expect(bulk).toHaveBeenCalledTimes(1));
    expect(bulk).toHaveBeenCalledWith([1, 2, 3], {
      genre: "Techno",
      comment: { mode: "append", text: "x" },
    });
    await waitFor(() => expect(useLibrary.getState().pages.get(0)?.[1]?.genre).toBe("Techno"));
    // The form is clean again.
    expect(screen.getByRole("button", { name: "Apply" })).toHaveProperty("disabled", true);
  });

  it("tags are added to or removed from all, never replaced", async () => {
    select(rows);
    bulk.mockResolvedValue({ rows, tagsAdded: 1, tagsRemoved: 0 });
    render(<BulkEditPanel />);
    const peak = screen.getByRole("checkbox", { name: /Peak/ });
    const warm = screen.getByRole("checkbox", { name: /Warm/ });
    expect(peak).toHaveProperty("checked", false);
    expect(peak).toHaveProperty("indeterminate", true);
    fireEvent.click(peak); // some have it: add to all
    fireEvent.click(warm);
    fireEvent.click(screen.getByRole("button", { name: /Apply to/ }));
    await waitFor(() => expect(bulk).toHaveBeenCalled());
    expect(bulk).toHaveBeenCalledWith([1, 2, 3], { addTags: [5, 6] });
  });

  it("Escape discards the edits", () => {
    select(rows);
    render(<BulkEditPanel />);
    const genre = screen.getByLabelText("Genre");
    fireEvent.change(genre, { target: { value: "Techno" } });
    fireEvent.keyDown(genre, { key: "Escape" });
    expect(genre).toHaveProperty("value", "House");
    expect(bulk).not.toHaveBeenCalled();
  });

  it("a rejected edit shows the error and keeps the form", async () => {
    select(rows);
    bulk.mockRejectedValue("invalid year: 0 is outside 1..=9999");
    render(<BulkEditPanel />);
    fireEvent.change(screen.getByLabelText("Year"), { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: /Apply to/ }));
    expect((await screen.findByRole("alert")).textContent).toContain("invalid year");
    expect(screen.getByLabelText("Year")).toHaveProperty("value", "0");
  });

  it("asks first for a large selection and applies only after confirming", async () => {
    const many = Array.from({ length: 25 }, (_, i) => row(i + 1));
    select(many);
    bulk.mockResolvedValue({ rows: many, tagsAdded: 0, tagsRemoved: 0 });
    render(<BulkEditPanel />);
    fireEvent.change(screen.getByLabelText("Genre"), { target: { value: "Techno" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply to 25 tracks" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("25");
    expect(dialog.textContent).toContain("cannot be undone");
    expect(bulk).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(bulk).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Apply to 25 tracks" }));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Change 25 tracks" }),
    );
    await waitFor(() => expect(bulk).toHaveBeenCalledTimes(1));
  });

  it("waits for pages of the selection that are not loaded yet", () => {
    useLibrary.setState({
      pages: new Map([[0, [row(1)]]]),
      total: 600,
      dividers: [],
      selection: { selected: new Set([0, 450]), anchor: 0, focus: 0 },
      pagesGeneration: 0,
      generation: 0,
    });
    render(<BulkEditPanel />);
    expect(screen.getByText(/Loading 1 selected tracks/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
  });
});

describe("TrackInfoPanel", () => {
  it("switches to the bulk form with two tracks selected", () => {
    select([row(1), row(2)]);
    render(<TrackInfoPanel />);
    expect(screen.getByText(/2 tracks selected/)).toBeTruthy();
  });
});
