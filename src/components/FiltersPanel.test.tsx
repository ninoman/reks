// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({ libraryFacets: vi.fn(), libraryQuery: vi.fn() }));

const ipc = await import("../lib/ipc");
const { noFilters, useLibrary } = await import("../store/library");
const { FiltersPanel } = await import("./FiltersPanel");

beforeEach(() => {
  vi.mocked(ipc.libraryQuery).mockResolvedValue({ rows: [], total: 0 });
  vi.mocked(ipc.libraryQuery).mockClear();
  useLibrary.setState({
    filters: { ...noFilters, playlistId: 4, genre: "House", ratingMin: 3 },
    filtersEnabled: true,
  });
});
afterEach(cleanup);

describe("FiltersPanel", () => {
  it("collapses to a strip that still counts active filters (not the playlist)", () => {
    render(<FiltersPanel />);
    expect(screen.getByLabelText("Minimum BPM")).toBeTruthy();
    fireEvent.click(screen.getByTitle("Hide filters"));
    expect(screen.queryByLabelText("Minimum BPM")).toBeNull();
    expect(screen.getByTitle("Show filters").textContent).toBe("Filters2");
    fireEvent.click(screen.getByTitle("Show filters"));
    expect(screen.getByLabelText("Minimum BPM")).toBeTruthy();
  });

  it("the switch turns filters off and on, independently of collapsing", () => {
    render(<FiltersPanel />);
    const toggle = () => screen.getByRole("switch", { name: "Apply filters" });
    fireEvent.click(toggle());
    expect(useLibrary.getState().filtersEnabled).toBe(false);
    // Values are kept; the query sends only the playlist.
    expect(useLibrary.getState().filters.genre).toBe("House");
    const sent = vi.mocked(ipc.libraryQuery).mock.lastCall?.[0];
    expect(sent?.filters).toEqual([{ type: "playlist", id: 4 }]);

    // Collapsing doesn't touch it, and the strip keeps the switch.
    fireEvent.click(screen.getByTitle("Hide filters"));
    expect(useLibrary.getState().filtersEnabled).toBe(false);
    expect(toggle().getAttribute("aria-checked")).toBe("false");
    fireEvent.click(toggle());
    expect(useLibrary.getState().filtersEnabled).toBe(true);
    expect(screen.queryByLabelText("Minimum BPM")).toBeNull();
    expect(vi.mocked(ipc.libraryQuery).mock.lastCall?.[0].filters).toHaveLength(3);
  });

  it("Clear keeps the shown playlist", () => {
    render(<FiltersPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(useLibrary.getState().filters).toEqual({ ...noFilters, playlistId: 4 });
  });
});
