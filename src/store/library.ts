/**
 * Library table state (Zustand): search text, the shown playlist, the Filters
 * drawer's rule (built in `rules.ts`), sort, column
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

export const defaultSort: Sort = { column: "dateAdded", descending: true };

/** Upper BPM bound when only a minimum is set (×100). */
export const BPM_X100_MAX = 1_000_00;

/** The parts of the store a query is built from. */
export interface QueryInputs {
  text: string;
  /** The playlist or folder chosen in the tree; `null`: the whole collection. */
  playlistId: number | null;
  /** Off: the Filters drawer's rule is kept but not applied (the playlist still is). */
  filtersEnabled: boolean;
  /** The Filters drawer's rule (`rules.ts`); `null` = nothing filters. */
  rule: RuleNode | null;
  sort: Sort;
}

/** Upper play count bound when only a minimum is set. */
export const PLAYS_MAX = 1_000_000;

/** The `TrackQuery` for page `page` of the current inputs. */
export function buildQuery(inputs: QueryInputs, page: number): TrackQuery {
  const text = inputs.text.trim();
  return {
    text: text === "" ? null : text,
    filters: inputs.playlistId === null ? [] : [{ type: "playlist", id: inputs.playlistId }],
    ...(inputs.filtersEnabled && inputs.rule !== null ? { rule: inputs.rule } : {}),
    sort: { ...inputs.sort },
    offset: page * PAGE_SIZE,
    limit: PAGE_SIZE,
  };
}

export interface LibraryState extends QueryInputs {
  columnVisibility: Record<string, boolean>;
  /** Column widths the user dragged to, px, by column id; others use their default. */
  columnWidths: Record<string, number>;
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
  /** `library_facets` failed (the pickers offer Retry); the table keeps working. */
  facetsError: string | null;
  error: string | null;

  setText(text: string): void;
  /** Applies the drawer's rule or sets it aside without losing it. */
  setFiltersEnabled(enabled: boolean): void;
  /** Sets the drawer's rule (`null`: none) and re-queries. */
  setRule(rule: RuleNode | null): void;
  /**
   * Shows a playlist (sorted by position), a folder (everything below it) or,
   * with `null`, the whole collection.
   */
  showPlaylist(id: number | null, isFolder: boolean): void;
  /** Header click: same column flips direction, another column sorts ascending. */
  toggleSort(column: Sort["column"]): void;
  setColumnVisibility(v: Record<string, boolean>): void;
  /** `null`: back to the column's default width. */
  setColumnWidth(id: string, width: number | null): void;
  setColumnWidths(widths: Record<string, number>): void;
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
      playlistId: null,
      filtersEnabled: true,
      rule: null,
      sort: defaultSort,
      columnVisibility: {},
      columnWidths: {},
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
      facetsError: null,
      error: null,

      setText(text) {
        set({ text });
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
      showPlaylist(id, isFolder) {
        set((s) => {
          let sort = s.sort;
          if (id !== null && !isFolder) sort = { column: "playlistPosition", descending: false };
          else if (sort.column === "playlistPosition") sort = defaultSort;
          return { playlistId: id, sort };
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
      setColumnWidth(id, width) {
        set((s) => {
          const { [id]: _, ...rest } = s.columnWidths;
          return { columnWidths: width === null ? rest : { ...rest, [id]: width } };
        });
      },
      setColumnWidths(columnWidths) {
        set({ columnWidths });
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
          set({ facets: await libraryFacets(), facetsError: null });
        } catch (e) {
          set({ facetsError: String(e) });
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
