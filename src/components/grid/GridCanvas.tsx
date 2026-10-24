/**
 * The grid editor's waveform: 3-band columns, beat lines, tempo section
 * markers (with their BPM), the selected section's band, transients (with
 * snap on), the playhead and the cursor. Presentational: pointer gestures
 * come back as callbacks — click places the cursor, dragging a marker moves
 * it, Alt-dragging a beat line stretches its section onto the release point.
 */
import { useEffect, useState } from "react";

import { formatBpm } from "../../lib/format";
import {
  bandPeaks,
  beatNear,
  markerNear,
  sectionAt,
  sectionViews,
  visibleBeats,
} from "../../lib/gridView";
import type { Beatgrid } from "../../lib/ipc";
import { useTheme } from "../../store/settings";

/** How near (px) a pointer must be to grab a marker or a beat line. */
const GRAB_PX = 5;

type Drag =
  | { kind: "marker"; section: number; ms: number }
  | { kind: "stretch"; section: number; beatMs: number; ms: number };

export interface GridCanvasProps {
  columns: Uint8Array | null;
  grid: Beatgrid | null;
  durationMs: number;
  selected: number | null;
  startMs: number;
  pxPerS: number;
  cursorMs: number | null;
  playheadMs: number | null;
  transients: number[] | null;
  /** Markers and beats can be dragged. */
  editable: boolean;
  onWidth: (px: number) => void;
  /** Wheel scroll by `deltaMs`. */
  onScroll: (deltaMs: number) => void;
  /** A click (no drag) at `ms`. */
  onPick: (ms: number) => void;
  onMoveMarker: (section: number, toMs: number) => void;
  onStretch: (section: number, beatMs: number, toMs: number) => void;
}

export function GridCanvas({
  columns,
  grid,
  durationMs,
  selected,
  startMs,
  pxPerS,
  cursorMs,
  playheadMs,
  transients,
  editable,
  onWidth,
  onScroll,
  onPick,
  onMoveMarker,
  onStretch,
}: GridCanvasProps) {
  // A state ref: the dialog portals its content in after the first render.
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null);
  const [width, setWidth] = useState(0);
  const [drag, setDrag] = useState<Drag | null>(null);
  const theme = useTheme();

  const msAt = (clientX: number, el: HTMLElement) =>
    Math.round(startMs + ((clientX - el.getBoundingClientRect().left) / pxPerS) * 1000);
  const grabMs = (GRAB_PX / pxPerS) * 1000;

  useEffect(() => {
    if (canvas === null) return;
    const observer = new ResizeObserver(([entry]) => {
      const w = Math.floor(entry?.contentRect.width ?? 0);
      setWidth(w);
      onWidth(w);
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [canvas, onWidth]);

  // Wheel scrolls the view (native listener: React's is passive).
  useEffect(() => {
    if (canvas === null) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      onScroll((delta / pxPerS) * 1000);
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, [canvas, pxPerS, onScroll]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `theme` redraws in the new colours.
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
    // Colours and sizes from tokens.css (the canvas can't use classes).
    const style = getComputedStyle(canvas);
    const token = (name: string) => style.getPropertyValue(name).trim();
    const mid = height / 2;
    const xOf = (ms: number) => ((ms - startMs) * pxPerS) / 1000;
    const anchors = grid?.anchors ?? [];

    if (selected !== null && anchors.length > 1) {
      const s = sectionViews(anchors, durationMs)[selected];
      if (s !== undefined) {
        ctx.fillStyle = token("--color-grid-section");
        ctx.fillRect(xOf(s.startMs), 0, xOf(s.endMs) - xOf(s.startMs), height);
      }
    }
    if (columns !== null) {
      const peaks = bandPeaks(columns, startMs, pxPerS, width);
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
    if (transients !== null) {
      ctx.fillStyle = token("--color-grid-transient");
      const tick = height / 12;
      for (const t of transients) {
        const x = xOf(t);
        if (x >= -1 && x <= width + 1) ctx.fillRect(Math.round(x), height - tick, 1, tick);
      }
    }
    if (grid !== null) {
      const beat = token("--color-grid-beat");
      const downbeat = token("--color-grid-downbeat");
      for (const b of visibleBeats(grid.beats, startMs, pxPerS, width)) {
        ctx.fillStyle = b.downbeat ? downbeat : beat;
        ctx.fillRect(Math.round(b.x), 0, b.downbeat ? 2 : 1, height);
      }
    }
    // Markers: a line, a flag with the section's tempo.
    const marker = token("--color-grid-marker");
    ctx.font = `${token("--text-2xs")} ${style.fontFamily}`;
    ctx.textBaseline = "top";
    const flagH = Number.parseFloat(token("--text-2xs")) + 4;
    anchors.forEach((a, i) => {
      const moving = drag?.kind === "marker" && drag.section === i;
      const x = Math.round(xOf(moving ? drag.ms : a.positionMs));
      if (x < -60 || x > width + 1) return;
      ctx.fillStyle = marker;
      ctx.fillRect(x - 1, 0, i === selected ? 3 : 2, height);
      const label = formatBpm(a.bpmX100);
      const w = ctx.measureText(label).width + 6;
      ctx.fillRect(x, 0, w, flagH);
      ctx.fillStyle = token("--color-deck");
      ctx.fillText(label, x + 3, 2);
    });
    if (drag?.kind === "stretch") {
      ctx.fillStyle = marker;
      ctx.globalAlpha = 0.5;
      ctx.fillRect(Math.round(xOf(drag.beatMs)), 0, 1, height);
      ctx.globalAlpha = 1;
      ctx.fillRect(Math.round(xOf(drag.ms)) - 1, 0, 2, height);
    }
    if (playheadMs !== null) {
      ctx.fillStyle = token("--color-deck-fg");
      ctx.fillRect(Math.round(xOf(playheadMs)), 0, 1, height);
    }
    if (cursorMs !== null) {
      ctx.fillStyle = token("--color-grid-cursor");
      ctx.fillRect(Math.round(xOf(cursorMs)) - 1, 0, 2, height);
    }
  }, [
    canvas,
    columns,
    grid,
    durationMs,
    selected,
    startMs,
    pxPerS,
    width,
    cursorMs,
    playheadMs,
    transients,
    drag,
    theme,
  ]);

  return (
    <canvas
      ref={setCanvas}
      className="h-40 w-full cursor-crosshair touch-none rounded-md bg-deck"
      role="img"
      aria-label="Waveform and beatgrid"
      onPointerDown={(e) => {
        const ms = msAt(e.clientX, e.currentTarget);
        const anchors = grid?.anchors ?? [];
        if (editable) {
          const m = e.altKey ? null : markerNear(anchors, ms, grabMs);
          const b = e.altKey && grid !== null ? beatNear(grid.beats, ms, grabMs) : null;
          if (m !== null) {
            setDrag({ kind: "marker", section: m, ms: anchors[m]?.positionMs ?? ms });
          } else if (b !== null) {
            const section = sectionAt(anchors, b) ?? 0;
            setDrag({ kind: "stretch", section, beatMs: b, ms: b });
          }
          if (m !== null || b !== null) {
            e.currentTarget.setPointerCapture?.(e.pointerId);
            return;
          }
        }
        onPick(ms);
      }}
      onPointerMove={(e) => {
        if (drag !== null) setDrag({ ...drag, ms: msAt(e.clientX, e.currentTarget) });
      }}
      onPointerUp={(e) => {
        if (drag === null) return;
        const ms = msAt(e.clientX, e.currentTarget);
        setDrag(null);
        if (drag.kind === "marker") {
          const from = grid?.anchors[drag.section]?.positionMs;
          if (from === undefined || from === ms) onPick(ms);
          else onMoveMarker(drag.section, ms);
        } else if (ms !== drag.beatMs) {
          onStretch(drag.section, drag.beatMs, ms);
        }
      }}
      onPointerCancel={() => setDrag(null)}
    />
  );
}
