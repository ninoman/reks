/**
 * Grid editor (phase 3 tasks 13, 18; D10 "Grid editing with tempo
 * sections"): the track's 3-band waveform from our own columns (150/s,
 * zoomable), its beat lines and tempo section markers, and the edits: shift
 * the grid, add/move/delete markers, a section's BPM typed or tapped, ½ and
 * ×2, beat 1 at the cursor, stretch a section onto a beat, reset, snap to
 * transients, undo/redo, lock. Every edit goes to Rust (`beatgrid_edit`),
 * which does the maths, refuses degenerate grids and stores the grid as
 * `manual`; this component draws and forwards.
 */
import { type KeyboardEvent, useCallback, useState } from "react";

import { formatBpm, formatPosition } from "../lib/format";
import { GRID_SHORTCUTS, gridKey } from "../lib/gridKeys";
import {
  columnsDurationMs,
  DEFAULT_ZOOM,
  sectionAt,
  sectionViews,
  ZOOM_PX_PER_S,
} from "../lib/gridView";
import type { GridEdit } from "../lib/ipc";
import { usePlayerPosition } from "../store/player";
import { ErrorMessage } from "./ErrorMessage";
import { GridCanvas } from "./grid/GridCanvas";
import { SectionControls } from "./grid/SectionControls";
import { SectionList } from "./grid/SectionList";
import { useGridEditor } from "./grid/useGridEditor";
import { useTap } from "./grid/useTap";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { Kbd } from "./ui/Kbd";
import { Slider } from "./ui/Slider";
import { Switch } from "./ui/Switch";

export function GridEditorDialog({
  trackId,
  title,
  onClose,
  onChanged,
}: {
  trackId: number;
  title: string;
  onClose: () => void;
  /** After every stored edit, so the caller can refresh its summary. */
  onChanged: () => void;
}) {
  const ed = useGridEditor(trackId, onChanged);
  const { bpmX100: tapBpm, tap } = useTap();
  const [zoom, setZoom] = useState(DEFAULT_ZOOM);
  const [startMs, setStartMs] = useState(0);
  const [cursorMs, setCursorMs] = useState<number | null>(null);
  const [width, setWidth] = useState(0);

  const { grid, selected } = ed;
  const anchors = grid?.anchors ?? [];
  const pxPerS = ZOOM_PX_PER_S[zoom] ?? 100;
  const durationMs = ed.columns === null ? 0 : columnsDurationMs(ed.columns);
  const viewMs = width > 0 ? (width / pxPerS) * 1000 : 0;
  const maxStart = Math.max(0, durationMs - viewMs);
  const start = Math.min(startMs, maxStart);
  const playheadMs = usePlayerPosition(trackId);
  const sections = sectionViews(anchors, durationMs);
  const hasGrid = anchors.length > 0;
  const locked = grid?.locked ?? false;
  const canEdit = grid !== null && !locked && !ed.busy;
  const section = selected === null ? undefined : anchors[selected];

  const edit = (e: GridEdit) => void ed.apply(e);

  function zoomTo(next: number) {
    const z = Math.min(ZOOM_PX_PER_S.length - 1, Math.max(0, next));
    const pps = ZOOM_PX_PER_S[z] ?? pxPerS;
    // Keep the cursor (or the middle of the view) where it is on screen.
    const focus = cursorMs ?? start + viewMs / 2;
    const offsetPx = ((focus - start) * pxPerS) / 1000;
    setZoom(z);
    setStartMs(Math.max(0, focus - (offsetPx / pps) * 1000));
  }

  function selectSection(i: number) {
    ed.select(i);
    // Bring its start into view.
    const s = sections[i];
    if (s !== undefined && (s.startMs < start || s.startMs > start + viewMs)) {
      setStartMs(Math.max(0, s.startMs - viewMs / 8));
    }
  }

  const onScroll = useCallback(
    (deltaMs: number) => setStartMs((s) => Math.min(maxStart, Math.max(0, s + deltaMs))),
    [maxStart],
  );

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const action = gridKey(e);
    if (action === null) return;
    e.preventDefault();
    e.stopPropagation();
    const at = cursorMs;
    switch (action.type) {
      case "undo":
        void ed.undo();
        return;
      case "redo":
        void ed.redo();
        return;
      case "zoom":
        zoomTo(zoom + action.step);
        return;
      case "snap":
        ed.setSnap(!ed.snap);
        return;
      case "tap":
        if (!locked) void tap();
        return;
      case "select":
        if (selected !== null) {
          selectSection(Math.min(anchors.length - 1, Math.max(0, selected + action.step)));
        }
        return;
    }
    if (!canEdit || !hasGrid) return;
    switch (action.type) {
      case "nudge":
        edit({ op: "nudge", deltaMs: action.deltaMs });
        break;
      case "moveMarker":
        if (section !== undefined && selected !== null) {
          edit({ op: "moveMarker", section: selected, toMs: section.positionMs + action.deltaMs });
        }
        break;
      case "addMarker":
        if (at !== null) edit({ op: "addMarker", atMs: at });
        break;
      case "deleteMarker":
        if (selected !== null && anchors.length > 1)
          edit({ op: "deleteMarker", section: selected });
        break;
      case "setDownbeat":
        if (at !== null) edit({ op: "setDownbeat", atMs: at });
        break;
      case "halve":
      case "double":
        if (selected !== null) edit({ op: action.type, section: selected });
        break;
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent size="xl" onKeyDown={onKeyDown}>
        <DialogHeader>
          <DialogTitle>Beatgrid · {title}</DialogTitle>
          <DialogDescription>
            Click the waveform to place the cursor; drag a marker to move it, Alt-drag a beat line
            to stretch its section. Edits are saved as a manual grid, which analysis keeps.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-lg font-semibold tabular-nums" data-testid="grid-bpm">
              {section !== undefined
                ? formatBpm(section.bpmX100)
                : hasGrid || grid === null
                  ? ""
                  : "No grid"}
            </span>
            {hasGrid && <span className="text-xs text-fg-muted">BPM</span>}
            {grid?.source != null && (
              <Badge variant={grid.source === "manual" ? "accent" : "neutral"}>{grid.source}</Badge>
            )}
            {anchors.length > 1 && <Badge variant="muted">{anchors.length} sections</Badge>}
            <span className="flex-1" />
            <span className="text-xs text-fg-muted tabular-nums">
              {cursorMs === null ? "No cursor" : `Cursor ${formatPosition(cursorMs)}`}
            </span>
            <Button
              size="sm"
              variant="outline"
              onClick={() => zoomTo(zoom - 1)}
              disabled={zoom === 0}
            >
              Zoom out
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => zoomTo(zoom + 1)}
              disabled={zoom === ZOOM_PX_PER_S.length - 1}
            >
              Zoom in
            </Button>
            {/* biome-ignore lint/a11y/noLabelWithoutControl: the Switch inside is the control. */}
            <label className="flex items-center gap-1.5 text-xs">
              <Switch
                checked={ed.snap}
                onCheckedChange={ed.setSnap}
                aria-label="Snap to transients"
              />
              Snap
            </label>
            {/* biome-ignore lint/a11y/noLabelWithoutControl: the Switch inside is the control. */}
            <label className="flex items-center gap-1.5 text-xs">
              <Switch
                checked={locked}
                disabled={grid === null || ed.busy}
                onCheckedChange={(on) => edit({ op: "setLocked", locked: on })}
                aria-label="Lock grid"
              />
              Lock grid
            </label>
          </div>

          {sections.length > 1 && (
            <SectionList sections={sections} selected={selected} onSelect={selectSection} />
          )}

          <GridCanvas
            columns={ed.columns}
            grid={grid}
            durationMs={durationMs}
            selected={selected}
            startMs={start}
            pxPerS={pxPerS}
            cursorMs={cursorMs}
            playheadMs={playheadMs}
            transients={ed.snap ? ed.transients : null}
            editable={canEdit}
            onWidth={setWidth}
            onScroll={onScroll}
            onPick={(ms) => {
              setCursorMs(ms);
              const s = sectionAt(anchors, ms);
              if (s !== null) ed.select(s);
            }}
            onMoveMarker={(s, toMs) => edit({ op: "moveMarker", section: s, toMs })}
            onStretch={(s, beatMs, toMs) => edit({ op: "stretch", section: s, beatMs, toMs })}
          />
          <Slider
            aria-label="Scroll"
            min={0}
            max={Math.max(1, Math.round(maxStart))}
            step={10}
            value={Math.round(start)}
            onValueChange={setStartMs}
            disabled={maxStart === 0}
          />
          {ed.columns === null && ed.error === null && (
            <p className="m-0 text-xs text-fg-muted">Computing the waveform…</p>
          )}

          <div className="flex flex-wrap items-center gap-1.5">
            <span className="mr-0.5 text-xs text-fg-muted">Shift grid</span>
            {[-10, -1, 1, 10].map((d) => (
              <Button
                key={d}
                size="sm"
                variant="outline"
                disabled={!canEdit || !hasGrid}
                onClick={() => edit({ op: "nudge", deltaMs: d })}
              >
                {d > 0 ? `+${d}` : `−${-d}`} ms
              </Button>
            ))}
            <span className="mx-1 h-4 w-px bg-border" />
            <Button
              size="sm"
              variant="outline"
              disabled={!canEdit || !hasGrid || cursorMs === null}
              onClick={() => cursorMs !== null && edit({ op: "addMarker", atMs: cursorMs })}
            >
              Add marker at cursor
            </Button>
            <span className="mx-1 h-4 w-px bg-border" />
            <Button
              size="sm"
              variant="outline"
              disabled={!ed.canUndo || ed.busy}
              onClick={() => void ed.undo()}
            >
              Undo
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={!ed.canRedo || ed.busy}
              onClick={() => void ed.redo()}
            >
              Redo
            </Button>
            <span className="flex-1" />
            <Button
              size="sm"
              variant="quiet"
              disabled={!canEdit || grid?.source !== "manual"}
              onClick={() => edit({ op: "reset", section: null })}
            >
              Reset grid
            </Button>
          </div>

          {section !== undefined && selected !== null ? (
            <SectionControls
              section={selected}
              count={anchors.length}
              bpmX100={section.bpmX100}
              cursorMs={cursorMs}
              tapBpmX100={tapBpm}
              disabled={!canEdit}
              resettable={grid?.source === "manual"}
              onEdit={edit}
              onTap={() => void tap()}
            />
          ) : (
            grid !== null && (
              <div className="flex flex-wrap items-center gap-1.5">
                <Button size="sm" variant="secondary" onClick={() => void tap()} disabled={locked}>
                  Tap
                </Button>
                <span className="min-w-12 text-xs tabular-nums" data-testid="tap-bpm">
                  {tapBpm === null ? "–" : formatBpm(tapBpm)}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={tapBpm === null || !canEdit}
                  onClick={() =>
                    tapBpm !== null &&
                    edit({ op: "setBpm", bpmX100: tapBpm, anchorMs: cursorMs ?? 0 })
                  }
                >
                  Create grid
                </Button>
              </div>
            )
          )}
          {ed.error !== null && <ErrorMessage message={ed.error} />}
          <ul className="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-2xs text-fg-muted">
            {GRID_SHORTCUTS.map(([keys, what]) => (
              <li key={keys} className="flex items-center gap-1">
                <Kbd>{keys}</Kbd>
                {what}
              </li>
            ))}
          </ul>
        </DialogBody>
        <DialogFooter>
          <Button variant="primary" onClick={onClose}>
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
