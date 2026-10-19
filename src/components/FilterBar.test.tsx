// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({ libraryFacets: vi.fn(), libraryQuery: vi.fn() }));

const ipc = await import("../lib/ipc");
const { noFilters, useLibrary } = await import("../store/library");
const { useSettings } = await import("../store/settings");
const { FilterBar, FiltersSwitch } = await import("./FilterBar");

function notation(keyNotation: "classical" | "camelot" | "openKey") {
  useSettings.setState({
    settings: {
      audioDevice: null,
      keyNotation,
      cacheDir: "/cache",
      ui: {
        hiddenColumns: null,
        filterBarOpen: true,
        filtersEnabled: true,
        infoOpen: false,
        volume: 1,
      },
    },
  });
}

beforeEach(() => {
  vi.mocked(ipc.libraryQuery).mockResolvedValue({ rows: [], total: 0 });
  vi.mocked(ipc.libraryQuery).mockClear();
  useLibrary.setState({
    filters: { ...noFilters, playlistId: 4, genre: "House", ratingMin: 3 },
    filtersEnabled: true,
    facets: {
      genres: ["House", "Techno"],
      keys: [
        { key: "Am", camelot: "8A", openKey: "1m" },
        { key: "C", camelot: "8B", openKey: "1d" },
      ],
      colours: ["red"],
    } as never,
  });
});
afterEach(cleanup);

describe("FilterBar", () => {
  it("outlines only the controls that filter", () => {
    render(<FilterBar />);
    const outlined = [
      ...document.querySelectorAll('[data-slot="field"][data-active] [data-slot="field-label"]'),
    ].map((e) => e.textContent);
    expect(outlined).toEqual(["Genre", "Rating"]);
  });

  it("picks keys from a popover and sums them up on its button", () => {
    notation("camelot");
    render(<FilterBar />);
    const button = screen.getByRole("button", { name: "Key" });
    expect(button.textContent).toBe("Any");
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("button", { name: "8A" }));
    expect(useLibrary.getState().filters.keys).toEqual(["Am"]);
    expect(button.textContent).toBe("8A");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("group", { name: "Keys" })).toBeNull();
  });

  it("labels keys in the notation chosen in Settings", () => {
    useLibrary.setState((s) => ({ filters: { ...s.filters, keys: ["Am"] } }));
    notation("openKey");
    render(<FilterBar />);
    const button = screen.getByRole("button", { name: "Key" });
    expect(button.textContent).toBe("1m");
    fireEvent.click(button);
    expect(screen.getByRole("button", { name: "1d" }).getAttribute("aria-pressed")).toBe("false");
    cleanup();
    notation("classical");
    render(<FilterBar />);
    expect(screen.getByRole("button", { name: "Key" }).textContent).toBe("Am");
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
