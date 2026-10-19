// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({ libraryFacets: vi.fn(), libraryQuery: vi.fn() }));

const ipc = await import("../lib/ipc");
const { noFilters, useLibrary } = await import("../store/library");
const { FiltersPanel } = await import("./FiltersPanel");

beforeEach(() => {
  vi.mocked(ipc.libraryQuery).mockResolvedValue({ rows: [], total: 0 });
  useLibrary.setState({ filters: { ...noFilters, playlistId: 4, genre: "House", ratingMin: 3 } });
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

  it("Clear keeps the shown playlist", () => {
    render(<FiltersPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(useLibrary.getState().filters).toEqual({ ...noFilters, playlistId: 4 });
  });
});
