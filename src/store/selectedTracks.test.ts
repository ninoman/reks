// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { TrackRow } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  libraryQuery: vi.fn(),
  libraryPages: vi.fn(),
  libraryFacets: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { PAGE_SIZE, useLibrary } = await import("./library");
const { useSelectedTracks } = await import("./selectedTracks");

const row = (id: number) => ({ id }) as TrackRow;
const page = (p: number) => Array.from({ length: PAGE_SIZE }, (_, i) => row(p * PAGE_SIZE + i + 1));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("useSelectedTracks", () => {
  it("fills every missing page of a select-all in one backend call", async () => {
    const total = PAGE_SIZE * 4;
    vi.mocked(ipc.libraryPages).mockResolvedValue(
      [1, 2, 3].map((p) => ({ page: p, rows: page(p) })),
    );
    useLibrary.setState({
      generation: 1,
      pagesGeneration: 1,
      total,
      dividers: [],
      pages: new Map([[0, page(0)]]),
      inFlight: new Set(),
      selection: {
        selected: new Set(Array.from({ length: total }, (_, i) => i)),
        anchor: 0,
        focus: 0,
      },
    });

    const { result } = renderHook(() => useSelectedTracks());
    expect(result.current.pending).toBe(PAGE_SIZE * 3);
    await waitFor(() => expect(result.current.pending).toBe(0));

    expect(ipc.libraryPages).toHaveBeenCalledTimes(1);
    expect(vi.mocked(ipc.libraryPages).mock.calls[0]?.[2]).toEqual([1, 2, 3]);
    expect(ipc.libraryQuery).not.toHaveBeenCalled();
    expect(result.current.rows).toHaveLength(total);
  });

  it("asks only for the pages the selected rows sit on", async () => {
    vi.mocked(ipc.libraryPages).mockResolvedValue([]);
    useLibrary.setState({
      generation: 1,
      pagesGeneration: 1,
      total: PAGE_SIZE * 4,
      dividers: [],
      pages: new Map([[0, page(0)]]),
      inFlight: new Set(),
      selection: {
        selected: new Set([0, PAGE_SIZE * 2 + 1, PAGE_SIZE * 3 + 5]),
        anchor: 0,
        focus: 0,
      },
    });
    renderHook(() => useSelectedTracks());
    await act(async () => {});
    expect(vi.mocked(ipc.libraryPages).mock.calls).toEqual([
      [expect.anything(), PAGE_SIZE, [2, 3]],
    ]);
  });
});
