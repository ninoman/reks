// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { FoundFile, RelocatePlan, RelocateScan, RelocateTrack } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  relocatePlan: vi.fn(),
  pickFolder: vi.fn(),
  relocateApply: vi.fn(),
  relocateCancel: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  playlistsGet: vi.fn(),
  libraryRoots: vi.fn(),
  scanStart: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useRelocate } = await import("../store/relocate");
const { RelocateDialog } = await import("./RelocateDialog");

const track = (id: number, label: string): RelocateTrack => ({
  id,
  path: `/old/${label}.flac`,
  label,
});
const file = (path: string, sameSize = true): FoundFile => ({
  path,
  size: 30_000_000,
  durationMs: 245_000,
  sameSize,
});

function plan(over: Partial<RelocatePlan> = {}): RelocatePlan {
  return {
    folder: "/Volumes/Archive",
    insideRoot: false,
    lookedFor: 4,
    filesSeen: 20,
    sure: [{ track: track(1, "Sure One"), file: file("/Volumes/Archive/a.flac") }],
    review: [
      {
        track: track(2, "Two Copies"),
        basis: "sameContent",
        candidates: [file("/Volumes/Archive/x/b.flac"), file("/Volumes/Archive/y/b.flac")],
      },
      {
        track: track(3, "Look Alike"),
        basis: "possible",
        candidates: [file("/Volumes/Archive/c.flac", false)],
      },
    ],
    notFound: [track(4, "Gone")],
    problems: [],
    cancelled: false,
    ...over,
  };
}

const scanOf = (p: RelocatePlan): RelocateScan => ({ planId: 7, plan: p });

beforeEach(() => {
  vi.mocked(ipc.relocatePlan).mockResolvedValue(scanOf(plan()));
  vi.mocked(ipc.relocateApply).mockResolvedValue({
    relinked: 2,
    skipped: [],
    root: null,
    cancelled: false,
  });
  vi.mocked(ipc.relocateCancel).mockResolvedValue(undefined);
  vi.mocked(ipc.libraryRoots).mockResolvedValue([]);
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [] });
});

afterEach(() => {
  cleanup();
  useRelocate.getState().close();
  vi.clearAllMocks();
});

async function openDialog() {
  void useRelocate.getState().start({ kind: "allMissing" }, "/Volumes/Archive");
  render(<RelocateDialog />);
  await screen.findByText(/4 missing tracks: 1 found/);
}

describe("RelocateDialog", () => {
  it("lists sure matches, picks and not-found, with no pick preselected", async () => {
    await openDialog();
    expect(ipc.relocatePlan).toHaveBeenCalledWith(
      { kind: "allMissing" },
      "/Volumes/Archive",
      expect.any(Function),
    );
    expect(screen.getByText("Sure One")).toBeTruthy();
    const copies = screen.getByLabelText("File for Two Copies") as HTMLSelectElement;
    expect(copies.value).toBe("");
    expect([...copies.options].map((o) => o.value)).toEqual([
      "",
      "/Volumes/Archive/x/b.flac",
      "/Volumes/Archive/y/b.flac",
    ]);
    expect(screen.getByText(/different size/)).toBeTruthy();
    expect(screen.getByText("Gone")).toBeTruthy();
    // 1 sure + 0 picks.
    expect(screen.getByRole("button", { name: "Relink 1 track" })).toBeTruthy();
  });

  it("applies the sure matches and the picks, with the add-folder tick", async () => {
    await openDialog();
    fireEvent.change(screen.getByLabelText("File for Two Copies"), {
      target: { value: "/Volumes/Archive/y/b.flac" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: /Add this folder/ }));
    fireEvent.click(screen.getByRole("button", { name: "Relink 2 tracks" }));
    await screen.findByText(/2 tracks relinked/);
    expect(ipc.relocateApply).toHaveBeenCalledWith(
      7,
      { picks: { 2: "/Volumes/Archive/y/b.flac" }, addRoot: true },
      expect.any(Function),
    );
    await waitFor(() => expect(ipc.libraryRoots).toHaveBeenCalled());
  });

  it("does not offer the folder as a library folder when a root covers it", async () => {
    vi.mocked(ipc.relocatePlan).mockResolvedValue(scanOf(plan({ insideRoot: true })));
    await openDialog();
    expect(screen.queryByRole("checkbox", { name: /Add this folder/ })).toBeNull();
  });

  it("with nothing found, Relink is disabled and another folder can be tried", async () => {
    vi.mocked(ipc.relocatePlan).mockResolvedValue(
      scanOf(plan({ sure: [], review: [], notFound: [track(4, "Gone")], lookedFor: 1 })),
    );
    void useRelocate.getState().start({ kind: "allMissing" }, "/Volumes/Archive");
    render(<RelocateDialog />);
    await screen.findByText(/1 missing track: 0 found, 1 not found/);
    expect((screen.getByRole("button", { name: "Relink" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(screen.getByRole("button", { name: "Try another folder…" })).toBeTruthy();
  });

  it("says so when none of the selected tracks is missing", async () => {
    vi.mocked(ipc.relocatePlan).mockResolvedValue(
      scanOf(plan({ lookedFor: 0, sure: [], review: [], notFound: [] })),
    );
    void useRelocate.getState().start({ kind: "tracks", ids: [1] }, "/Volumes/Archive");
    render(<RelocateDialog />);
    await screen.findByText("None of these tracks is missing.");
  });

  it("shows a planning error and only offers Close", async () => {
    vi.mocked(ipc.relocatePlan).mockRejectedValue("folder not readable");
    void useRelocate.getState().start({ kind: "allMissing" }, "/nope");
    render(<RelocateDialog />);
    await screen.findByText("folder not readable");
    expect(screen.queryByRole("button", { name: /Relink/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(useRelocate.getState().step).toBe("closed");
  });

  it("keeps the dry run on screen when the apply is refused", async () => {
    vi.mocked(ipc.relocateApply).mockRejectedValue("this dry run is out of date");
    await openDialog();
    fireEvent.click(screen.getByRole("button", { name: "Relink 1 track" }));
    await screen.findByText("this dry run is out of date");
    expect(useRelocate.getState().step).toBe("plan");
  });

  it("stops a running dry run", async () => {
    vi.mocked(ipc.relocatePlan).mockReturnValue(new Promise(() => {}));
    void useRelocate.getState().start({ kind: "allMissing" }, "/Volumes/Archive");
    render(<RelocateDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    expect(ipc.relocateCancel).toHaveBeenCalled();
  });
});
