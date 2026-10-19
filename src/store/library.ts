/**
 * Library table state (Zustand): search text, sidebar filters, sort, column
 * visibility, the loaded pages of the current result, and the selection.
 * Rows are fetched a page at a time as the table scrolls; the WebView never
 * holds the whole library.
 */
import { create } from "zustand";

import {
  type Facets,
  type Filter,
  type Key,
  libraryFacets,
  libraryQuery,
  type Sort,
  type TrackQuery,
  type TrackRow,
} from "../lib/ipc";
import {
  emptySelection,
  type Selection,
  type SelectionAction,
  selectionReducer,
} from "./selection";

/** Rows per `library_query` call. */
export const PAGE_SIZE = 200;

/** Sidebar filter values; `null` / empty = not filtering. */
export interface FilterState {
  genre: string | null;
  keys: Key[];
  /** BPM as shown (e.g. 128), not × 100. */
  bpmMin: number | null;
  bpmMax: number | null;
  color: string | null;
  /** 0 = any. */
  ratingMin: number;
  playlistId: number | null;
  missing: boolean | null;
  needsAnalysis: boolean | null;
}

export const noFilters: FilterState = {
  genre: null,
  keys: [],
  bpmMin: null,
  bpmMax: null,
  color: null,
  ratingMin: 0,
  playlistId: null,
  missing: null,
  needsAnalysis: null,
};

export const defaultSort: Sort = { column: "title", descending: false };

/** Upper BPM bound when only a minimum is set (×100). */
const BPM_X100_MAX = 1_000_00;

/** The parts of the store a query is built from. */
export interface QueryInputs {
  text: string;
  filters: FilterState;
  sort: Sort;
}

/** Sidebar state → the `Filter` list `library_query` takes. */
export function buildFilters(f: FilterState): Filter[] {
  const out: Filter[] = [];
  if (f.genre !== null) out.push({ type: "genre", genre: f.genre });
  if (f.keys.length > 0) out.push({ type: "keyIn", keys: [...f.keys] });
  if (f.bpmMin !== null || f.bpmMax !== null) {
    out.push({
      type: "bpmRange",
      min: f.bpmMin === null ? 0 : Math.round(f.bpmMin * 100),
      max: f.bpmMax === null ? BPM_X100_MAX : Math.round(f.bpmMax * 100),
    });
  }
  if (f.color !== null) out.push({ type: "color", color: f.color });
  if (f.ratingMin > 0) out.push({ type: "ratingMin", rating: f.ratingMin });
  if (f.playlistId !== null) out.push({ type: "playlist", id: f.playlistId });
  if (f.missing !== null) out.push({ type: "missing", missing: f.missing });
  if (f.needsAnalysis !== null) out.push({ type: "needsAnalysis", needs: f.needsAnalysis });
  return out;
}

/** The `TrackQuery` for page `page` of the current inputs. */
export function buildQuery(inputs: QueryInputs, page: number): TrackQuery {
  const text = inputs.text.trim();
  return {
    text: text === "" ? null : text,
    filters: buildFilters(inputs.filters),
    sort: { ...inputs.sort },
    offset: page * PAGE_SIZE,
    limit: PAGE_SIZE,
  };
}

export interface LibraryState extends QueryInputs {
  columnVisibility: Record<string, boolean>;
  /** Matching tracks over all pages. */
  total: number;
  /** Loaded pages of the current result, by page index. */
  pages: ReadonlyMap<number, TrackRow[]>;
  /** Bumped on every query change; responses for older generations are dropped. */
  generation: number;
  /** Generation the loaded `pages` and `total` belong to. */
  pagesGeneration: number;
  inFlight: ReadonlySet<number>;
  selection: Selection;
  facets: Facets | null;
  error: string | null;

  setText(text: string): void;
  setFilters(patch: Partial<FilterState>): void;
  /** Clears the sidebar filters; the shown playlist (the tree's selection) stays. */
  clearFilters(): void;
  /**
   * Shows a playlist (sorted by position), a folder (everything below it) or,
   * with `null`, the whole collection.
   */
  showPlaylist(id: number | null, isFolder: boolean): void;
  /** Header click: same column flips direction, another column sorts ascending. */
  toggleSort(column: Sort["column"]): void;
  setColumnVisibility(v: Record<string, boolean>): void;
  /** Re-runs the query from page 0 (after a query change or an import). */
  refresh(): void;
  /** Loads every page overlapping rows `start..=end` that isn't loaded yet. */
  ensureRange(start: number, end: number): void;
  select(action: SelectionAction): void;
  /**
   * Swaps in a changed row (after an edit) wherever it is loaded, without
   * re-querying: selection and scroll stay put, and a row that no longer
   * matches the filter or sort stays where it is until the next refresh.
   */
  replaceRow(row: TrackRow): void;
  loadFacets(): Promise<void>;
}

export const useLibrary = create<LibraryState>()((set, get) => {
  async function fetchPage(page: number, generation: number): Promise<void> {
    set((s) => ({ inFlight: new Set(s.inFlight).add(page) }));
    try {
      const result = await libraryQuery(buildQuery(get(), page));
      if (get().generation !== generation) return;
      set((s) => {
        // Keep showing the old result until the new one's first page lands.
        const pages = new Map(s.pagesGeneration === generation ? s.pages : []);
        pages.set(page, result.rows);
        return { pages, total: result.total, pagesGeneration: generation, error: null };
      });
    } catch (e) {
      if (get().generation === generation) set({ error: String(e) });
    } finally {
      if (get().generation === generation) {
        set((s) => {
          const inFlight = new Set(s.inFlight);
          inFlight.delete(page);
          return { inFlight };
        });
      }
    }
  }

  return {
    text: "",
    filters: noFilters,
    sort: defaultSort,
    columnVisibility: {},
    total: 0,
    pages: new Map(),
    generation: 0,
    pagesGeneration: -1,
    inFlight: new Set(),
    selection: emptySelection,
    facets: null,
    error: null,

    setText(text) {
      set({ text });
      get().refresh();
    },
    setFilters(patch) {
      set((s) => ({ filters: { ...s.filters, ...patch } }));
      get().refresh();
    },
    clearFilters() {
      set((s) => ({ filters: { ...noFilters, playlistId: s.filters.playlistId } }));
      get().refresh();
    },
    showPlaylist(id, isFolder) {
      set((s) => {
        let sort = s.sort;
        if (id !== null && !isFolder) sort = { column: "playlistPosition", descending: false };
        else if (sort.column === "playlistPosition") sort = defaultSort;
        return { filters: { ...s.filters, playlistId: id }, sort };
      });
      get().refresh();
    },
    toggleSort(column) {
      set((s) => ({
        sort: { column, descending: s.sort.column === column ? !s.sort.descending : false },
      }));
      get().refresh();
    },
    setColumnVisibility(columnVisibility) {
      set({ columnVisibility });
    },
    refresh() {
      const generation = get().generation + 1;
      set({ generation, inFlight: new Set(), selection: emptySelection });
      void fetchPage(0, generation);
    },
    ensureRange(start, end) {
      const s = get();
      // Until the current generation's first page arrives, rows belong to the old result.
      if (s.pagesGeneration !== s.generation) return;
      const first = Math.max(0, Math.floor(start / PAGE_SIZE));
      const last = Math.floor(Math.max(start, end) / PAGE_SIZE);
      for (let p = first; p <= last; p++) {
        if (p * PAGE_SIZE >= s.total) break;
        if (!s.pages.has(p) && !s.inFlight.has(p)) void fetchPage(p, s.generation);
      }
    },
    select(action) {
      set((s) => ({ selection: selectionReducer(s.selection, action) }));
    },
    replaceRow(row) {
      set((s) => {
        let changed = false;
        const pages = new Map(s.pages);
        for (const [p, rows] of pages) {
          if (!rows.some((r) => r.id === row.id)) continue;
          // A playlist view can hold the track more than once; each row keeps its entry.
          pages.set(
            p,
            rows.map((r) => (r.id === row.id ? { ...row, entryPosition: r.entryPosition } : r)),
          );
          changed = true;
        }
        return changed ? { pages } : {};
      });
    },
    async loadFacets() {
      try {
        set({ facets: await libraryFacets() });
      } catch (e) {
        set({ error: String(e) });
      }
    },
  };
});

/** Row at `index` of the current result, if its page is loaded. */
export function rowAt(pages: ReadonlyMap<number, TrackRow[]>, index: number): TrackRow | undefined {
  return pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
}
