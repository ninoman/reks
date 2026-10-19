/**
 * The virtualised track table. Only the visible rows (plus overscan) are in
 * the DOM; rows come from the store's page cache, which fetches pages from
 * `library_query` as they scroll into view.
 *
 * Rows drag (the whole selection) onto playlists in the tree. In a playlist
 * view sorted by position (ascending) they also drag to reorder, and Delete or
 * the context menu removes the selected entries.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import {
  type DragEvent,
  type KeyboardEvent,
  type MouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { hasType, readTracks, TRACKS_MIME, type TrackDrag, zoneAt } from "../lib/dnd";
import { rowAt, useLibrary } from "../store/library";
import { shownPlaylist, usePlaylists } from "../store/playlists";
import { ContextMenu, type MenuState } from "./ContextMenu";
import type { ColumnSpec } from "./columns";
import { ROW_HEIGHT } from "./trackTableModel";

export function TrackTable({ columns }: { columns: ColumnSpec[] }) {
  const total = useLibrary((s) => s.total);
  const pages = useLibrary((s) => s.pages);
  const pagesGeneration = useLibrary((s) => s.pagesGeneration);
  const sort = useLibrary((s) => s.sort);
  const selection = useLibrary((s) => s.selection);
  const toggleSort = useLibrary((s) => s.toggleSort);
  const ensureRange = useLibrary((s) => s.ensureRange);
  const select = useLibrary((s) => s.select);
  const playlistId = useLibrary((s) => s.filters.playlistId);
  const nodes = usePlaylists((s) => s.nodes);
  const view = shownPlaylist(nodes, playlistId);
  const canReorder = view !== null && sort.column === "playlistPosition" && !sort.descending;
  const [menu, setMenu] = useState<MenuState | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [dropAt, setDropAt] = useState<{ index: number; after: boolean } | null>(null);
  // A plain press on a selected row selects only it on release, unless it starts a drag.
  const pendingClick = useRef<number | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);
  // Plain React (no compiler), so the virtualizer's unmemoisable API is fine here.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
    // Keep keyboard-focused rows clear of the sticky header (row height + border).
    scrollPaddingStart: ROW_HEIGHT + 1,
  });
  const items = virtualizer.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new total or result generation must re-check the visible range.
  useEffect(() => {
    ensureRange(first, last);
  }, [first, last, total, pagesGeneration, ensureRange]);

  // A new result starts at the top.
  // biome-ignore lint/correctness/useExhaustiveDependencies: pagesGeneration is the trigger, not an input.
  useEffect(() => {
    virtualizer.scrollToOffset(0);
  }, [pagesGeneration, virtualizer]);

  const width = useMemo(() => columns.reduce((w, c) => w + c.width, 0), [columns]);

  function moveFocus(delta: number, extend: boolean) {
    select({ type: "move", delta, extend, count: total });
    const focus = useLibrary.getState().selection.focus;
    if (focus !== null) virtualizer.scrollToIndex(focus, { align: "auto" });
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const pageRows = Math.max(
      1,
      Math.floor((scrollRef.current?.clientHeight ?? ROW_HEIGHT) / ROW_HEIGHT) - 1,
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
      select({ type: "selectAll", count: total });
    } else if (e.key === "Escape") {
      select({ type: "clear" });
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

  /** The selected rows that are loaded, in table order. */
  function selectedRows() {
    const current = useLibrary.getState();
    return [...current.selection.selected]
      .sort((a, b) => a - b)
      .map((i) => rowAt(current.pages, i))
      .filter((r) => r !== undefined);
  }

  function removeSelected() {
    if (view === null) return;
    const positions = selectedRows()
      .map((r) => r.entryPosition)
      .filter((p) => p !== null);
    void usePlaylists.getState().removeEntries(view.id, positions);
  }

  function onRowDragStart(e: DragEvent<HTMLElement>, index: number) {
    pendingClick.current = null;
    if (!useLibrary.getState().selection.selected.has(index)) select({ type: "click", index });
    const rows = selectedRows();
    const payload: TrackDrag = {
      trackIds: rows.map((r) => r.id),
      playlistId: view?.id ?? null,
      entryPositions: rows.map((r) => r.entryPosition).filter((p) => p !== null),
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
    const row = rowAt(pages, index);
    if (!canReorder || drag === null || drag.playlistId !== view?.id || row?.entryPosition == null)
      return;
    void usePlaylists
      .getState()
      .reorderEntries(view.id, drag.entryPositions, row.entryPosition + (after ? 1 : 0));
  }

  function onRowContextMenu(e: MouseEvent, index: number) {
    e.preventDefault();
    if (view === null) return;
    if (!selection.selected.has(index)) select({ type: "click", index });
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: [{ label: `Remove from ${view.name}`, onSelect: removeSelected }],
    });
  }

  return (
    <div
      ref={scrollRef}
      className="track-table"
      role="grid"
      aria-rowcount={total}
      aria-multiselectable
      tabIndex={0}
      onKeyDown={onKeyDown}
    >
      <div className="tt-header" role="row" style={{ width }}>
        {columns.map((c) => (
          <div
            key={c.id}
            role="columnheader"
            className={`tt-cell${c.align === "right" ? " right" : ""}${c.sort ? " sortable" : ""}`}
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
          </div>
        ))}
      </div>
      <div className="tt-body" style={{ height: virtualizer.getTotalSize(), width }}>
        {items.map((item) => {
          const row = rowAt(pages, item.index);
          const selected = selection.selected.has(item.index);
          const focused = selection.focus === item.index;
          return (
            <div
              key={item.index}
              role="row"
              aria-rowindex={item.index + 1}
              aria-selected={selected}
              className={`tt-row${item.index % 2 === 1 ? " odd" : ""}${selected ? " selected" : ""}${focused ? " focused" : ""}${
                row === undefined ? " loading" : ""
              }${row?.missing ? " missing" : ""}${
                dropAt?.index === item.index ? (dropAt.after ? " drop-after" : " drop-before") : ""
              }`}
              style={{ transform: `translateY(${item.start}px)`, height: ROW_HEIGHT }}
              draggable={row !== undefined}
              onMouseDown={(e) => onRowMouseDown(e, item.index)}
              onClick={() => {
                if (pendingClick.current === item.index)
                  select({ type: "click", index: item.index });
                pendingClick.current = null;
              }}
              onContextMenu={(e) => onRowContextMenu(e, item.index)}
              onDragStart={(e) => onRowDragStart(e, item.index)}
              onDragOver={(e) => onRowDragOver(e, item.index)}
              onDragLeave={() => setDropAt((d) => (d?.index === item.index ? null : d))}
              onDrop={(e) => onRowDrop(e, item.index)}
              onDragEnd={() => setDropAt(null)}
            >
              {columns.map((c) => (
                <div
                  key={c.id}
                  role="gridcell"
                  className={`tt-cell${c.align === "right" ? " right" : ""}`}
                  style={{ width: c.width }}
                >
                  {row === undefined ? null : c.cell(row)}
                </div>
              ))}
            </div>
          );
        })}
      </div>
      {menu !== null && <ContextMenu menu={menu} onClose={closeMenu} />}
    </div>
  );
}
