/**
 * Library table state (Zustand): search text, sidebar filters, sort, column
 * visibility, the loaded pages of the current result, and the selection.
 * Rows are fetched a page at a time as the table scrolls; the WebView never
 * holds the whole library.
 *
 * Table rows are the tracks plus, in a playlist in playlist order, its
 * dividers (D22), which every page carries in full. Selection and the
 * virtualiser count table rows; pages hold tracks only. `itemAt` maps one to
 * the other.
 */
import { create, type StoreApi, type UseBoundStore } from "zustand";

import {
  type DividerRow,
  type Facets,
  type Filter,
  type Key,
  libraryFacets,
  libraryQuery,
  type RuleNode,
  type Sort,
  type TagId,
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
  /** Play count range (D23: baseline plus counted sessions). */
  playsMin: number | null;
  playsMax: number | null;
  /** "never", or played within this many days. */
  lastPlayed: LastPlayedFilter | null;
  /** My Tags: tracks with at least one of these. */
  tagIds: TagId[];
}

/** The "Last played" filter's choices. */
export type LastPlayedFilter = "never" | 30 | 90 | 365;

export const noFilters: FilterState = {
  genre: null,
  keys: [],
  bpmMin: null,
  bpmMax: null,
  color: null,
  ratingMin: 0,
  playlistId: null,
  missing: null,
  playsMin: null,
  playsMax: null,
  lastPlayed: null,
  tagIds: [],
};

export const defaultSort: Sort = { column: "title", descending: false };

/** Upper BPM bound when only a minimum is set (×100). */
export const BPM_X100_MAX = 1_000_00;

/** The parts of the store a query is built from. */
export interface QueryInputs {
  text: string;
  filters: FilterState;
  /** Off: the sidebar filters are kept but not applied (the playlist still is). */
  filtersEnabled: boolean;
  /** The rule builder's finished rows, ANDed with the filters; `null` = none. */
  rule: RuleNode | null;
  sort: Sort;
}

/** Upper play count bound when only a minimum is set. */
export const PLAYS_MAX = 1_000_000;

/** Local calendar date `days` days before `now`, as `YYYY-MM-DD`. */
export function daysAgo(days: number, now: Date = new Date()): string {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - days);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Sidebar state → the `Filter` list `library_query` takes. */
export function buildFilters(f: FilterState, now: Date = new Date()): Filter[] {
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
  if (f.playsMin !== null || f.playsMax !== null) {
    out.push({ type: "playCountRange", min: f.playsMin ?? 0, max: f.playsMax ?? PLAYS_MAX });
  }
  if (f.lastPlayed === "never") out.push({ type: "neverPlayed" });
  else if (f.lastPlayed !== null)
    out.push({ type: "playedSince", date: daysAgo(f.lastPlayed, now) });
  if (f.tagIds.length > 0) out.push({ type: "tagAny", ids: [...f.tagIds] });
  return out;
}

/** Filters set in the sidebar panel (the shown playlist doesn't count). */
export function activeFilterCount(f: FilterState): number {
  return buildFilters({ ...f, playlistId: null }).length;
}

/** The `TrackQuery` for page `page` of the current inputs. */
export function buildQuery(inputs: QueryInputs, page: number): TrackQuery {
  const text = inputs.text.trim();
  return {
    text: text === "" ? null : text,
    filters: buildFilters(
      inputs.filtersEnabled
        ? inputs.filters
        : { ...noFilters, playlistId: inputs.filters.playlistId },
    ),
    ...(inputs.filtersEnabled && inputs.rule !== null ? { rule: inputs.rule } : {}),
    sort: { ...inputs.sort },
    offset: page * PAGE_SIZE,
    limit: PAGE_SIZE,
  };
}

export interface LibraryState extends QueryInputs {
  columnVisibility: Record<string, boolean>;
  /** Matching tracks over all pages (dividers not counted). */
  total: number;
  /** The shown playlist's dividers, when `dividerView`. */
  dividers: DividerRow[];
  /** The result shows dividers (playlist order, no search or other filter). */
  dividerView: boolean;
  /** Loaded pages of the current result, by page index. */
  pages: ReadonlyMap<number, TrackRow[]>;
  /** Bumped on every query change; responses for older generations are dropped. */
  generation: number;
  /** Generation the loaded `pages` and `total` belong to. */
  pagesGeneration: number;
  /** The current generation is an edit of the same view: the table keeps its place. */
  keepPlace: boolean;
  inFlight: ReadonlySet<number>;
  selection: Selection;
  facets: Facets | null;
  error: string | null;

  setText(text: string): void;
  setFilters(patch: Partial<FilterState>): void;
  /** Applies the filters or sets them aside without losing their values. */
  setFiltersEnabled(enabled: boolean): void;
  /** Sets the rule builder's rule (`null`: none) and re-queries. */
  setRule(rule: RuleNode | null): void;
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
  /**
   * Re-runs the query from page 0 (after a query change or an import).
   * `keepPlace`: the same view after an edit, so the table keeps its scroll.
   */
  refresh(keepPlace?: boolean): void;
  /** Loads every page overlapping table rows `start..=end` that isn't loaded yet. */
  ensureRange(start: number, end: number): void;
  select(action: SelectionAction): void;
  /**
   * Swaps in a changed row (after an edit) wherever it is loaded, without
   * re-querying: selection and scroll stay put, and a row that no longer
   * matches the filter or sort stays where it is until the next refresh.
   */
  replaceRow(row: TrackRow): void;
  /** Rewrites the tags of the loaded rows of `trackIds` (after an assign), keeping selection and scroll. */
  patchRowTags(trackIds: readonly number[], update: (tags: TagId[]) => TagId[]): void;
  loadFacets(): Promise<void>;
}

/** One track list's state; the main view and each split pane (`panes.ts`) has its own. */
export type LibraryStore = UseBoundStore<StoreApi<LibraryState>>;

export function createLibraryStore(): LibraryStore {
  return create<LibraryState>()((set, get) => {
    async function fetchPage(page: number, generation: number): Promise<void> {
      set((s) => ({ inFlight: new Set(s.inFlight).add(page) }));
      try {
        const result = await libraryQuery(buildQuery(get(), page));
        if (get().generation !== generation) return;
        set((s) => {
          // Keep showing the old result until the new one's first page lands.
          const pages = new Map(s.pagesGeneration === generation ? s.pages : []);
          pages.set(page, result.rows);
          return {
            pages,
            total: result.total,
            dividers: result.dividers,
            dividerView: result.dividerView,
            pagesGeneration: generation,
            error: null,
          };
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
      filtersEnabled: true,
      rule: null,
      sort: defaultSort,
      columnVisibility: {},
      total: 0,
      dividers: [],
      dividerView: false,
      pages: new Map(),
      generation: 0,
      pagesGeneration: -1,
      keepPlace: false,
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
      setFiltersEnabled(filtersEnabled) {
        if (get().filtersEnabled === filtersEnabled) return;
        set({ filtersEnabled });
        get().refresh();
      },
      setRule(rule) {
        set({ rule });
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
      refresh(keepPlace = false) {
        const generation = get().generation + 1;
        set({ generation, keepPlace, inFlight: new Set(), selection: emptySelection });
        void fetchPage(0, generation);
      },
      ensureRange(start, end) {
        const s = get();
        // Until the current generation's first page arrives, rows belong to the old result.
        if (s.pagesGeneration !== s.generation) return;
        const first = Math.max(0, Math.floor(trackIndex(s, start) / PAGE_SIZE));
        const last = Math.floor(trackIndex(s, Math.max(start, end)) / PAGE_SIZE);
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
      patchRowTags(trackIds, update) {
        const ids = new Set(trackIds);
        set((s) => {
          const pages = new Map(s.pages);
          for (const [p, rows] of pages) {
            if (rows.some((r) => ids.has(r.id)))
              pages.set(
                p,
                rows.map((r) => (ids.has(r.id) ? { ...r, tags: update(r.tags) } : r)),
              );
          }
          return { pages };
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
}

/** The main track list: the one the toolbar, filters and info panel work on. */
export const useLibrary = createLibraryStore();

/** The loaded part of the current result that table rows map onto. */
export interface LoadedView {
  pages: ReadonlyMap<number, TrackRow[]>;
  total: number;
  dividers: readonly DividerRow[];
}

/** One table row: a track entry or a divider. */
export type ViewItem = { kind: "track"; row: TrackRow } | { kind: "divider"; divider: DividerRow };

/** Table rows: tracks plus dividers. */
export function viewCount(v: LoadedView): number {
  return v.total + v.dividers.length;
}

/**
 * Table row of each divider, in order. Entry positions are dense (reks-core
 * rewrites them on every edit), so `position - k` tracks precede divider `k`;
 * the clamp keeps rows in range if they ever are not.
 */
function dividerSlots(v: LoadedView): number[] {
  let before = 0;
  return v.dividers.map((d, k) => {
    before = Math.min(v.total, Math.max(before, d.position - k));
    return before + k;
  });
}

/** Track index of table row `index`; a divider row gives the track after it. */
export function trackIndex(v: LoadedView, index: number): number {
  let n = 0;
  for (const slot of dividerSlots(v)) {
    if (slot >= index) break;
    n++;
  }
  return index - n;
}

/** Table row `index`: a divider, or a track if its page is loaded. */
export function itemAt(v: LoadedView, index: number): ViewItem | undefined {
  const slots = dividerSlots(v);
  let n = 0;
  for (const [k, slot] of slots.entries()) {
    if (slot === index) return { kind: "divider", divider: v.dividers[k] as DividerRow };
    if (slot > index) break;
    n++;
  }
  const t = index - n;
  const row = v.pages.get(Math.floor(t / PAGE_SIZE))?.[t % PAGE_SIZE];
  return row === undefined ? undefined : { kind: "track", row };
}

/** The track at table row `index`, if it is a track and its page is loaded. */
export function rowAt(v: LoadedView, index: number): TrackRow | undefined {
  const item = itemAt(v, index);
  return item?.kind === "track" ? item.row : undefined;
}

/** Entry position of a table row in a playlist view (tracks and dividers alike). */
export function entryPosition(item: ViewItem): number | null {
  return item.kind === "divider" ? item.divider.position : item.row.entryPosition;
}
