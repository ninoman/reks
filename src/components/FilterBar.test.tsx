// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({ libraryFacets: vi.fn(), libraryQuery: vi.fn() }));

const ipc = await import("../lib/ipc");
const { noFilters, useLibrary } = await import("../store/library");
const { FilterBar, FiltersSwitch } = await import("./FilterBar");

beforeEach(() => {
  vi.mocked(ipc.libraryQuery).mockResolvedValue({ rows: [], total: 0 });
  vi.mocked(ipc.libraryQuery).mockClear();
  useLibrary.setState({
    filters: { ...noFilters, playlistId: 4, genre: "House", ratingMin: 3 },
    filtersEnabled: true,
    facets: {
      genres: ["House", "Techno"],
      keys: [
        { key: "Am", camelot: "8A" },
        { key: "C", camelot: "8B" },
      ],
      colours: ["red"],
    } as never,
  });
});
afterEach(cleanup);

describe("FilterBar", () => {
  it("outlines only the controls that filter", () => {
    render(<FilterBar />);
    const outlined = [...document.querySelectorAll(".filter-field.set .filter-label")].map(
      (e) => e.textContent,
    );
    expect(outlined).toEqual(["Genre", "Rating"]);
  });

  it("picks keys from a popover and sums them up on its button", () => {
    render(<FilterBar />);
    const button = screen.getByRole("button", { name: "Key" });
    expect(button.textContent).toBe("Any ▾");
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("button", { name: "8A" }));
    expect(useLibrary.getState().filters.keys).toEqual(["Am"]);
    expect(button.textContent).toBe("8A ▾");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("group", { name: "Keys" })).toBeNull();
  });

  it("the switch turns filters off and on, keeping their values", () => {
    render(<FiltersSwitch />);
    const toggle = () => screen.getByRole("switch", { name: "Apply filters" });
    fireEvent.click(toggle());
    expect(useLibrary.getState().filtersEnabled).toBe(false);
    expect(toggle().getAttribute("aria-checked")).toBe("false");
    // Values are kept; the query sends only the playlist.
    expect(useLibrary.getState().filters.genre).toBe("House");
    const sent = vi.mocked(ipc.libraryQuery).mock.lastCall?.[0];
    expect(sent?.filters).toEqual([{ type: "playlist", id: 4 }]);

    fireEvent.click(toggle());
    expect(useLibrary.getState().filtersEnabled).toBe(true);
    expect(vi.mocked(ipc.libraryQuery).mock.lastCall?.[0].filters).toHaveLength(3);
  });

  it("Clear all keeps the shown playlist", () => {
    render(<FilterBar />);
    fireEvent.click(screen.getByRole("button", { name: "Clear all" }));
    expect(useLibrary.getState().filters).toEqual({ ...noFilters, playlistId: 4 });
  });
});
