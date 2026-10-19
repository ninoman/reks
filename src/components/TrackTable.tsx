/**
 * The virtualised track table. Only the visible rows (plus overscan) are in
 * the DOM; rows come from the store's page cache, which fetches pages from
 * `library_query` as they scroll into view.
 */
import { useEffect, useMemo, useRef, type KeyboardEvent, type MouseEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import { rowAt, useLibrary } from "../store/library";
import { ROW_HEIGHT } from "./trackTableModel";
import type { ColumnSpec } from "./columns";

export function TrackTable({ columns }: { columns: ColumnSpec[] }) {
  const total = useLibrary((s) => s.total);
  const pages = useLibrary((s) => s.pages);
  const pagesGeneration = useLibrary((s) => s.pagesGeneration);
  const sort = useLibrary((s) => s.sort);
  const selection = useLibrary((s) => s.selection);
  const toggleSort = useLibrary((s) => s.toggleSort);
  const ensureRange = useLibrary((s) => s.ensureRange);
  const select = useLibrary((s) => s.select);

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

  useEffect(() => {
    ensureRange(first, last);
  }, [first, last, total, pagesGeneration, ensureRange]);

  // A new result starts at the top.
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
    }
  }

  function onRowClick(e: MouseEvent, index: number) {
    if (e.shiftKey) select({ type: "range", index });
    else if (e.metaKey || e.ctrlKey) select({ type: "toggle", index });
    else select({ type: "click", index });
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
              }${row?.missing ? " missing" : ""}`}
              style={{ transform: `translateY(${item.start}px)`, height: ROW_HEIGHT }}
              onMouseDown={(e) => onRowClick(e, item.index)}
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
    </div>
  );
}
