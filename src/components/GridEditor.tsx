/**
 * Grid editor (Phase 3 task 13): the track's 3-band waveform from our own
 * columns (150/s, zoomable), its beat lines, and the edits: nudge ±1/±10 ms,
 * set the downbeat at the cursor, double/halve, tap tempo, lock. Every edit
 * goes to Rust (`beatgrid_edit`), which does the maths and stores the grid
 * as `manual`; this component only draws and forwards clicks.
 */
import { useCallback, useEffect, useState } from "react";

import { formatBpm, formatPosition } from "../lib/format";
import {
  bandPeaks,
  columnsDurationMs,
  DEFAULT_ZOOM,
  visibleBeats,
  ZOOM_PX_PER_S,
} from "../lib/gridView";
import {
  type Beatgrid,
  beatgridEdit,
  beatgridGet,
  beatgridTapTempo,
  type GridEdit,
  waveformColumns,
} from "../lib/ipc";
import { usePlayer } from "../store/player";
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
import { Slider } from "./ui/Slider";
import { Switch } from "./ui/Switch";

/** Taps kept for the tempo (Rust uses the newest run of these). */
const MAX_TAPS = 16;

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
  const [grid, setGrid] = useState<Beatgrid | null>(null);
  const [columns, setColumns] = useState<Uint8Array | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [zoom, setZoom] = useState(DEFAULT_ZOOM);
  const [startMs, setStartMs] = useState(0);
  const [cursorMs, setCursorMs] = useState<number | null>(null);
  const [taps, setTaps] = useState<number[]>([]);
  const [tapBpm, setTapBpm] = useState<number | null>(null);
  const [width, setWidth] = useState(0);
  // A state ref: the dialog portals its content in after the first render.
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null);

  const pxPerS = ZOOM_PX_PER_S[zoom] ?? 100;
  const durationMs = columns === null ? 0 : columnsDurationMs(columns);
  const viewMs = width > 0 ? (width / pxPerS) * 1000 : 0;
  const maxStart = Math.max(0, durationMs - viewMs);
  const playheadMs = usePlayer((s) =>
    s.status.trackId === trackId && s.status.loaded ? s.status.positionMs : null,
  );

  useEffect(() => {
    let live = true;
    beatgridGet(trackId).then(
      (g) => live && setGrid(g),
      (e) => live && setError(String(e)),
    );
    // Computed on demand the first time (a few seconds for a long track).
    waveformColumns(trackId).then(
      (b) => live && setColumns(new Uint8Array(b)),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [trackId]);

  const apply = useCallback(
    async (edit: GridEdit) => {
      setBusy(true);
      setError(null);
      try {
        setGrid(await beatgridEdit(trackId, edit));
        onChanged();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [trackId, onChanged],
  );

  async function tap() {
    const next = [...taps, performance.now()].slice(-MAX_TAPS);
    setTaps(next);
    setTapBpm(await beatgridTapTempo(next));
  }

  function zoomTo(next: number) {
    const z = Math.min(ZOOM_PX_PER_S.length - 1, Math.max(0, next));
    const pps = ZOOM_PX_PER_S[z] ?? pxPerS;
    // Keep the cursor (or the middle of the view) where it is on screen.
    const focus = cursorMs ?? startMs + viewMs / 2;
    const offsetPx = ((focus - startMs) * pxPerS) / 1000;
    setZoom(z);
    setStartMs(Math.max(0, focus - (offsetPx / pps) * 1000));
  }

  // Width of the canvas in CSS px.
  useEffect(() => {
    if (canvas === null) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.floor(entry?.contentRect.width ?? 0)),
    );
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [canvas]);

  // Wheel scrolls the view (native listener: React's is passive).
  useEffect(() => {
    if (canvas === null) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      setStartMs((s) => Math.min(maxStart, Math.max(0, s + (delta / pxPerS) * 1000)));
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, [canvas, maxStart, pxPerS]);

  const start = Math.min(startMs, maxStart);

  useEffect(() => {
    if (canvas === null || width === 0) return;
    const ctx = canvas.getContext("2d");
    if (ctx === null) return;
    const dpr = window.devicePixelRatio || 1;
    const height = canvas.clientHeight;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    // Colours from tokens.css (the canvas can't use classes).
    const style = getComputedStyle(canvas);
    const token = (name: string) => style.getPropertyValue(name).trim();
    const mid = height / 2;
    const xOf = (ms: number) => ((ms - start) * pxPerS) / 1000;

    if (columns !== null) {
      const peaks = bandPeaks(columns, start, pxPerS, width);
      const bands: [number, string][] = [
        [0, token("--color-waveform-low")],
        [1, token("--color-waveform-mid")],
        [2, token("--color-waveform-high")],
      ];
      for (const [band, colour] of bands) {
        ctx.fillStyle = colour;
        for (let x = 0; x < width; x++) {
          const h = (peaks[3 * x + band] ?? 0) * mid * (band === 2 ? 0.6 : 1);
          if (h > 0.5) ctx.fillRect(x, mid - h, 1, 2 * h);
        }
      }
    }
    if (grid !== null) {
      const beat = token("--color-grid-beat");
      const downbeat = token("--color-grid-downbeat");
      for (const b of visibleBeats(grid.beats, start, pxPerS, width)) {
        ctx.fillStyle = b.downbeat ? downbeat : beat;
        ctx.fillRect(Math.round(b.x), 0, b.downbeat ? 2 : 1, height);
      }
    }
    if (playheadMs !== null) {
      ctx.fillStyle = token("--color-deck-fg");
      ctx.fillRect(Math.round(xOf(playheadMs)), 0, 1, height);
    }
    if (cursorMs !== null) {
      ctx.fillStyle = token("--color-grid-cursor");
      ctx.fillRect(Math.round(xOf(cursorMs)) - 1, 0, 2, height);
    }
  }, [canvas, columns, grid, start, pxPerS, width, cursorMs, playheadMs]);

  const anchor = grid?.anchors[0] ?? null;
  const hasGrid = anchor !== null;
  const dynamic = (grid?.anchors.length ?? 0) > 1;
  const locked = grid?.locked ?? false;
  const canEdit = grid !== null && !locked && !busy;
  const canConst = canEdit && hasGrid && !dynamic;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent size="xl">
        <DialogHeader>
          <DialogTitle>Beatgrid · {title}</DialogTitle>
          <DialogDescription>
            Click the waveform to place the cursor. Edits are saved as a manual grid, which
            re-analysis keeps.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-lg font-semibold tabular-nums" data-testid="grid-bpm">
              {hasGrid
                ? dynamic
                  ? `${formatBpm(anchor.bpmX100)}+`
                  : formatBpm(anchor.bpmX100)
                : "No grid"}
            </span>
            <span className="text-xs text-fg-muted">BPM</span>
            {grid?.source != null && (
              <Badge variant={grid.source === "manual" ? "accent" : "neutral"}>{grid.source}</Badge>
            )}
            {dynamic && <Badge variant="muted">dynamic: nudge only</Badge>}
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
                checked={locked}
                disabled={grid === null || busy}
                onCheckedChange={(on) => void apply({ op: "setLocked", locked: on })}
                aria-label="Lock grid"
              />
              Lock grid
            </label>
          </div>

          <canvas
            ref={setCanvas}
            className="h-40 w-full cursor-crosshair rounded-md bg-deck"
            role="img"
            aria-label="Waveform and beatgrid"
            onPointerDown={(e) => {
              const rect = e.currentTarget.getBoundingClientRect();
              setCursorMs(Math.round(start + ((e.clientX - rect.left) / pxPerS) * 1000));
            }}
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
          {columns === null && error === null && (
            <p className="m-0 text-xs text-fg-muted">Computing the waveform…</p>
          )}

          <div className="flex flex-wrap items-center gap-1.5">
            <span className="mr-0.5 text-xs text-fg-muted">Nudge</span>
            {[-10, -1, 1, 10].map((d) => (
              <Button
                key={d}
                size="sm"
                variant="outline"
                disabled={!canEdit || !hasGrid}
                onClick={() => void apply({ op: "nudge", deltaMs: d })}
              >
                {d > 0 ? `+${d}` : `−${-d}`} ms
              </Button>
            ))}
            <span className="mx-1 h-4 w-px bg-border" />
            <Button
              size="sm"
              variant="outline"
              disabled={!canConst || cursorMs === null}
              onClick={() => cursorMs !== null && void apply({ op: "setDownbeat", atMs: cursorMs })}
            >
              Set downbeat at cursor
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={!canConst}
              onClick={() => void apply({ op: "halve" })}
            >
              Halve BPM
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={!canConst}
              onClick={() => void apply({ op: "double" })}
            >
              Double BPM
            </Button>
            <span className="mx-1 h-4 w-px bg-border" />
            <Button size="sm" variant="secondary" onClick={() => void tap()} disabled={locked}>
              Tap
            </Button>
            <span className="min-w-14 text-xs tabular-nums" data-testid="tap-bpm">
              {tapBpm === null ? "–" : formatBpm(tapBpm)}
            </span>
            <Button
              size="sm"
              variant="outline"
              disabled={tapBpm === null || !canEdit || dynamic}
              onClick={() =>
                tapBpm !== null &&
                void apply({ op: "setBpm", bpmX100: tapBpm, anchorMs: cursorMs ?? 0 })
              }
            >
              {hasGrid ? "Use tapped BPM" : "Create grid"}
            </Button>
          </div>
          {error !== null && (
            <div className="text-danger select-text" role="alert">
              {error}
            </div>
          )}
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
