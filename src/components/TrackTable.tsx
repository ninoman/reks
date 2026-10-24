/**
 * The virtualised track table of the list in `LibraryStoreContext` (the main
 * list or a split pane). Only the visible rows (plus overscan) are in
 * the DOM; rows come from the store's page cache, which fetches pages from
 * `library_query` as they scroll into view.
 *
 * Rows drag (the whole selection) onto playlists in the tree. In a playlist
 * view sorted by position (ascending) they also drag to reorder, rows dropped
 * from another list (a split pane) are added at the drop line, and Delete or
 * the context menu removes the selected entries. With no search or other
 * filter that view also shows the playlist's dividers (D22) as full-width
 * rows, which select, drag and delete like entries; the context menu inserts
 * one above the selection, and double-click, Enter or the menu renames one.
 * The context menu converts the selected tracks anywhere (`ConvertDialog`)
 * and loads the first selected track onto a mixer deck (opening the mix panel).
 * Double-click or Enter loads
 * a row into the player and plays it; the loaded track's row is marked with
 * an accent bar and accent text, separate from the selection background.
 * Header edges drag to resize a column (widths shared by every list).
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import {
  type DragEvent,
  type KeyboardEvent,
  type MouseEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { hasType, readTracks, TRACKS_MIME, type TrackDrag, zoneAt } from "../lib/dnd";
import { shownDecks } from "../lib/mixView";
import { useAnalysis } from "../store/analysis";
import { useConverter } from "../store/converter";
import { entryPosition, itemAt, useLibrary, type ViewItem, viewCount } from "../store/library";
import { libraryRef, useMix } from "../store/mix";
import { usePanels } from "../store/panels";
import { activateTrack, usePlayerTrackId } from "../store/player";
import { shownPlaylist, usePlaylists } from "../store/playlists";
import { primaryIndex } from "../store/selection";
import { useTags } from "../store/tags";
import type { ColumnSpec } from "./columns";
import { DividerLabel } from "./DividerLabel";
import { useLibraryStore } from "./libraryStoreContext";
import { findMissingFiles } from "./relocateActions";
import { TagSubmenu } from "./TagSubmenu";
import {
  COLUMN_MAX_WIDTH,
  COLUMN_MIN_WIDTH,
  fitColumnWidths,
  ROW_HEIGHTS,
  sizedColumns,
} from "./trackTableModel";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "./ui/ContextMenu";
import { ResizeHandle } from "./ui/ResizeHandle";

/*
 * Row and cell classes are constant strings; per-row state is data
 * attributes, so a scroll re-render builds no class names.
 */
const CELL =
  "flex-none overflow-hidden text-ellipsis whitespace-nowrap px-1.5 h-(--row-h) leading-(--row-h)";
const CELL_RIGHT = `${CELL} text-right`;
const HEAD_CELL = `${CELL} relative data-[sort]:cursor-pointer data-[sort]:hover:text-fg`;
const HEAD_CELL_RIGHT = `${HEAD_CELL} text-right`;
const ROW = [
  "absolute inset-x-0 top-0 flex cursor-default items-center",
  "data-[odd]:bg-row-alt data-[selected]:bg-selected data-[missing]:text-fg-muted",
  // The track in the player: accent bar and text, apart from the selection
  // background and still visible when both apply.
  "data-[loaded]:font-medium data-[loaded]:text-accent data-[loaded]:data-[selected]:text-fg",
  "data-[loaded]:before:absolute data-[loaded]:before:inset-y-0 data-[loaded]:before:left-0 data-[loaded]:before:w-[3px] data-[loaded]:before:bg-accent",
  "group-focus:data-[focused]:shadow-[inset_0_0_0_1px_var(--color-focus-ring)]",
  "data-[drop=before]:shadow-[inset_0_2px_0_var(--color-accent)]",
  "data-[drop=after]:shadow-[inset_0_-2px_0_var(--color-accent)]",
  "data-[divider]:bg-bg-subtle data-[divider]:data-[selected]:bg-selected",
].join(" ");

export function TrackTable({ columns: visible }: { columns: ColumnSpec[] }) {
  const useLib = useLibraryStore();
  const total = useLib((s) => s.total);
  const dividers = useLib((s) => s.dividers);
  const dividerView = useLib((s) => s.dividerView);
  const pages = useLib((s) => s.pages);
  const pagesGeneration = useLib((s) => s.pagesGeneration);
  const sort = useLib((s) => s.sort);
  const selection = useLib((s) => s.selection);
  const toggleSort = useLib((s) => s.toggleSort);
  const ensureRange = useLib((s) => s.ensureRange);
  const select = useLib((s) => s.select);
  const playlistId = useLib((s) => s.playlistId);
  const loadedId = usePlayerTrackId();
  const deckCount = useMix((s) => s.deckCount);
  const density = usePanels((s) => s.rowDensity);
  const rowHeight = ROW_HEIGHTS[density];
  const dragged = useLibrary((s) => s.columnWidths);
  const setColumnWidth = useLibrary((s) => s.setColumnWidth);
  const specs = useMemo(
    () =>
      sizedColumns(
        density === "compact" ? visible : visible.filter((c) => c.compactOnly !== true),
        dragged,
      ),
    [visible, density, dragged],
  );
  // The My Tags column reads names from the tree: redraw when it changes.
  useTags((s) => s.tree);
  const nodes = usePlaylists((s) => s.nodes);
  const rename = usePlaylists((s) => s.dividerRename);
  const view = shownPlaylist(nodes, playlistId);
  const loaded = { pages, total, dividers };
  const count = viewCount(loaded);
  const canReorder =
    view !== null && !view.smart && sort.column === "playlistPosition" && !sort.descending;
  const [dropAt, setDropAt] = useState<{ index: number; after: boolean } | null>(null);
  // A plain press on a selected row selects only it on release, unless it starts a drag.
  const pendingClick = useRef<number | null>(null);
  // The menu choice starts a rename: the closing menu must not take focus back from its input.
  const menuRenames = useRef(false);

  const scrollRef = useRef<HTMLDivElement>(null);
  // Plain React (no compiler), so the virtualizer's unmemoisable API is fine here.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 20,
    // Keep keyboard-focused rows clear of the sticky header (one compact row + border).
    scrollPaddingStart: ROW_HEIGHTS.compact + 1,
  });
  const items = virtualizer.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;

  // biome-ignore lint/correctness/useExhaustiveDependencies: rows are re-measured when their height changes.
  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new count or result generation must re-check the visible range.
  useEffect(() => {
    ensureRange(first, last);
  }, [first, last, count, pagesGeneration, ensureRange]);

  // A new result starts at the top; an edit of the shown playlist keeps the place, and
  // a divider just inserted is scrolled to, where its rename input waits.
  // biome-ignore lint/correctness/useExhaustiveDependencies: pagesGeneration is the trigger, not an input.
  useEffect(() => {
    const { keepPlace, selection: sel } = useLib.getState();
    if (usePlaylists.getState().dividerRename !== null && sel.focus !== null)
      virtualizer.scrollToIndex(sel.focus, { align: "auto" });
    else if (!keepPlace) virtualizer.scrollToOffset(0);
  }, [pagesGeneration, virtualizer]);

  // Spare container width is shared out by `fitColumnWidths`; a wider table scrolls.
  const [available, setAvailable] = useState(0);
  useEffect(() => {
    const el = scrollRef.current;
    if (el === null) return;
    const observer = new ResizeObserver(() => setAvailable(el.clientWidth));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const widths = useMemo(() => fitColumnWidths(specs, available), [specs, available]);
  const columns = useMemo(
    () => specs.map((c, i) => ({ ...c, width: widths[i] ?? c.width })),
    [specs, widths],
  );
  const width = useMemo(() => widths.reduce((w, x) => w + x, 0), [widths]);

  function moveFocus(delta: number, extend: boolean) {
    select({ type: "move", delta, extend, count });
    const focus = useLib.getState().selection.focus;
    if (focus !== null) virtualizer.scrollToIndex(focus, { align: "auto" });
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const pageRows = Math.max(
      1,
      Math.floor((scrollRef.current?.clientHeight ?? rowHeight) / rowHeight) - 1,
    );
    const moves: Record<string, number> = {
      ArrowDown: 1,
      ArrowUp: -1,
      PageDown: pageRows,
      PageUp: -pageRows,
      End: Infinity,
      Home: -Infinity,
    };
    const delta = moves[e.key];
    if (delta !== undefined) {
      e.preventDefault();
      moveFocus(delta, e.shiftKey);
    } else if (e.key.toLowerCase() === "a" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      select({ type: "selectAll", count });
    } else if (e.key === "Escape") {
      select({ type: "clear" });
    } else if (e.key === "Enter") {
      const focus = useLib.getState().selection.focus;
      const item = focus === null ? undefined : itemAt(loaded, focus);
      if (item !== undefined) activate(item);
    } else if ((e.key === "Delete" || e.key === "Backspace") && view !== null) {
      e.preventDefault();
      removeSelected();
    }
  }

  function onRowMouseDown(e: MouseEvent, index: number) {
    pendingClick.current = null;
    // Right-click selection is the context menu's job.
    if (e.button !== 0) return;
    const plain = !e.shiftKey && !e.metaKey && !e.ctrlKey;
    if (plain && selection.selected.has(index)) {
      pendingClick.current = index;
      return;
    }
    if (e.shiftKey) select({ type: "range", index });
    else if (e.metaKey || e.ctrlKey) select({ type: "toggle", index });
    else select({ type: "click", index });
  }

  /** Double-click or Enter: a track plays (or loads onto a spare deck while mixing), a divider is renamed. */
  function activate(item: ViewItem) {
    if (item.kind === "track") void activateTrack(libraryRef(item.row));
    else if (view !== null)
      usePlaylists
        .getState()
        .startDividerRename(view.id, item.divider.position, item.divider.label);
  }

  /** The selected rows that are loaded (tracks and dividers), in table order. */
  function selectedItems() {
    const current = useLib.getState();
    return [...current.selection.selected]
      .sort((a, b) => a - b)
      .map((i) => itemAt(current, i))
      .filter((r) => r !== undefined);
  }

  /** Library ids of the selected tracks (dividers left out), in table order. */
  function selectedTrackIds() {
    return selectedItems().flatMap((i) => (i.kind === "track" ? [i.row.id] : []));
  }

  /** Entry positions of the selected rows, dividers included. */
  function selectedPositions() {
    return selectedItems()
      .map(entryPosition)
      .filter((p) => p !== null);
  }

  /** Loads the first selected track onto `deck` and shows the mix panel. */
  function loadToDeck(deck: number) {
    const first = selectedItems().find((i) => i.kind === "track");
    if (first?.kind !== "track") return;
    usePanels.getState().setMixOpen(true);
    void useMix.getState().load(deck, libraryRef(first.row));
  }

  function removeSelected() {
    if (view === null || view.smart) return;
    void usePlaylists.getState().removeEntries(view.id, selectedPositions());
  }

  /** Above the first selected row, or at the end. Table rows are entry positions here. */
  function insertDivider() {
    if (view === null || !dividerView) return;
    const { selected } = useLib.getState().selection;
    const position = selected.size === 0 ? count : Math.min(...selected);
    void usePlaylists.getState().insertDivider(view.id, position);
  }

  /** The divider the context menu would rename: the primary selected row, if a divider. */
  const primary = primaryIndex(selection);
  const primaryItem = primary === null ? undefined : itemAt(loaded, primary);
  const menuDivider = primaryItem?.kind === "divider" ? primaryItem.divider : null;

  function onRowDragStart(e: DragEvent<HTMLElement>, index: number) {
    pendingClick.current = null;
    if (!useLib.getState().selection.selected.has(index)) select({ type: "click", index });
    const items = selectedItems();
    const payload: TrackDrag = {
      trackIds: items.flatMap((r) => (r.kind === "track" ? [r.row.id] : [])),
      playlistId: view?.id ?? null,
      entryPositions: items.map(entryPosition).filter((p) => p !== null),
    };
    e.dataTransfer.setData(TRACKS_MIME, JSON.stringify(payload));
    e.dataTransfer.effectAllowed = "copyMove";
  }

  function onRowDragOver(e: DragEvent<HTMLElement>, index: number) {
    if (!canReorder || !hasType(e.dataTransfer, TRACKS_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rect = e.currentTarget.getBoundingClientRect();
    setDropAt({ index, after: zoneAt(e.clientY - rect.top, rect.height, false) === "after" });
  }

  function onRowDrop(e: DragEvent<HTMLElement>, index: number) {
    e.preventDefault();
    const after = dropAt?.index === index && dropAt.after;
    setDropAt(null);
    const drag = readTracks(e.dataTransfer);
    const item = itemAt(loaded, index);
    const target = item === undefined ? null : entryPosition(item);
    if (!canReorder || drag === null || view === null || target === null) return;
    const before = target + (after ? 1 : 0);
    // From another list (a split pane): add there instead of reordering.
    if (drag.playlistId === view.id)
      void usePlaylists.getState().reorderEntries(view.id, drag.entryPositions, before);
    else void usePlaylists.getState().addTracks(view.id, drag.trackIds, before);
  }

  /** Right-click selects the row first; the menu opens on the table. */
  function onRowContextMenu(index: number) {
    if (!selection.selected.has(index)) select({ type: "click", index });
  }

  return (
    <ContextMenu modal={false}>
      <ContextMenuTrigger asChild>
        <div
          ref={scrollRef}
          className="group relative flex-1 overflow-auto outline-none [contain:strict]"
          role="grid"
          aria-rowcount={count}
          aria-multiselectable
          tabIndex={0}
          onKeyDown={onKeyDown}
        >
          <div
            className="sticky top-0 z-(--z-sticky) flex border-b border-border bg-bg-subtle text-xs font-semibold text-fg-muted"
            role="row"
            style={{ width }}
          >
            {columns.map((c) => (
              <div
                key={c.id}
                role="columnheader"
                className={c.align === "right" ? HEAD_CELL_RIGHT : HEAD_CELL}
                data-sort={c.sort !== null || undefined}
                style={{ width: c.width }}
                aria-sort={
                  c.sort !== null && sort.column === c.sort
                    ? sort.descending
                      ? "descending"
                      : "ascending"
                    : undefined
                }
                onClick={
                  c.sort === null
                    ? undefined
                    : () => toggleSort(c.sort as NonNullable<ColumnSpec["sort"]>)
                }
              >
                {c.header}
                {c.sort !== null && sort.column === c.sort ? (sort.descending ? " ▼" : " ▲") : ""}
                <ResizeHandle
                  label={`Resize ${c.header || c.id} column`}
                  min={COLUMN_MIN_WIDTH}
                  max={COLUMN_MAX_WIDTH}
                  value={c.width}
                  onResize={(w) => setColumnWidth(c.id, w)}
                  onReset={() => setColumnWidth(c.id, null)}
                />
              </div>
            ))}
          </div>
          <div
            className="relative"
            data-density={density}
            style={{ height: virtualizer.getTotalSize(), width }}
          >
            {items.map((item) => {
              const entry = itemAt(loaded, item.index);
              const row = entry?.kind === "track" ? entry.row : undefined;
              const divider = entry?.kind === "divider" ? entry.divider : null;
              const draft =
                divider !== null &&
                rename !== null &&
                rename.playlistId === view?.id &&
                rename.position === divider.position
                  ? rename.draft
                  : null;
              const selected = selection.selected.has(item.index);
              const focused = selection.focus === item.index;
              const playing = row !== undefined && row.id === loadedId;
              return (
                <div
                  key={item.index}
                  role="row"
                  aria-rowindex={item.index + 1}
                  aria-selected={selected}
                  className={ROW}
                  data-odd={item.index % 2 === 1 || undefined}
                  data-selected={selected || undefined}
                  data-focused={focused || undefined}
                  data-loaded={playing || undefined}
                  data-loading={entry === undefined || undefined}
                  data-divider={divider !== null || undefined}
                  data-missing={row?.missing || undefined}
                  data-drop={
                    dropAt?.index === item.index ? (dropAt.after ? "after" : "before") : undefined
                  }
                  style={{ transform: `translateY(${item.start}px)`, height: rowHeight }}
                  draggable={entry !== undefined && draft === null}
                  onMouseDown={(e) => onRowMouseDown(e, item.index)}
                  onDoubleClick={() => {
                    if (entry !== undefined) activate(entry);
                  }}
                  onClick={() => {
                    if (pendingClick.current === item.index)
                      select({ type: "click", index: item.index });
                    pendingClick.current = null;
                  }}
                  onContextMenu={() => onRowContextMenu(item.index)}
                  onDragStart={(e) => onRowDragStart(e, item.index)}
                  onDragOver={(e) => onRowDragOver(e, item.index)}
                  onDragLeave={() => setDropAt((d) => (d?.index === item.index ? null : d))}
                  onDrop={(e) => onRowDrop(e, item.index)}
                  onDragEnd={() => setDropAt(null)}
                >
                  {divider !== null ? (
                    <div role="gridcell" aria-colspan={columns.length} className="flex flex-1">
                      <DividerLabel divider={divider} draft={draft} />
                    </div>
                  ) : (
                    columns.map((c) => (
                      <div
                        key={c.id}
                        role="gridcell"
                        className={c.align === "right" ? CELL_RIGHT : CELL}
                        style={{ width: c.width }}
                      >
                        {row === undefined ? null : c.cell(row, density)}
                      </div>
                    ))
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent
        onCloseAutoFocus={(e) => {
          if (menuRenames.current) e.preventDefault();
          menuRenames.current = false;
        }}
      >
        {shownDecks(deckCount).map((deck) => (
          <ContextMenuItem
            key={deck}
            disabled={selectedTrackIds().length === 0}
            onSelect={() => loadToDeck(deck)}
          >
            Load to deck {deck + 1}
          </ContextMenuItem>
        ))}
        <ContextMenuSeparator />
        <TagSubmenu trackIds={selectedTrackIds()} />
        <ContextMenuItem
          disabled={selectedTrackIds().length === 0}
          onSelect={() => useConverter.getState().open(selectedTrackIds())}
        >
          Convert…
        </ContextMenuItem>
        <ContextMenuItem
          disabled={selectedTrackIds().length === 0}
          onSelect={() => void findMissingFiles({ kind: "tracks", ids: selectedTrackIds() })}
        >
          Find missing files in…
        </ContextMenuItem>
        <ContextMenuItem
          disabled={selectedTrackIds().length === 0}
          onSelect={() => void useAnalysis.getState().reanalyse(selectedTrackIds())}
        >
          Re-analyse
        </ContextMenuItem>
        {view !== null && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem
              disabled={!dividerView || view.smart}
              onSelect={() => {
                menuRenames.current = true;
                insertDivider();
              }}
            >
              Insert divider
              {view.smart && <span className="text-fg-subtle"> (not in smart playlists)</span>}
            </ContextMenuItem>
            {menuDivider !== null && (
              <ContextMenuItem
                onSelect={() => {
                  menuRenames.current = true;
                  usePlaylists
                    .getState()
                    .startDividerRename(view.id, menuDivider.position, menuDivider.label);
                }}
              >
                Rename divider
              </ContextMenuItem>
            )}
            <ContextMenuSeparator />
            <ContextMenuItem disabled={view.smart} onSelect={removeSelected}>
              Remove from {view.name}
              {view.smart && <span className="text-fg-subtle"> (follows its rules)</span>}
            </ContextMenuItem>
          </>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
