/**
 * A track's whole waveform in a table cell, drawn as rekordbox draws its
 * colour preview, with the hot cues flagged at their positions. Empty while
 * it loads or when the file can't be read. A click plays the track from
 * there on the preview deck; while it plays there, a playhead shows where.
 */
import { type MouseEvent, useEffect, useMemo, useRef, useState } from "react";

import { formatRgb, hotCueLabel } from "../lib/format";
import type { HotCueMark } from "../lib/ipc";
import { previewPalette, previewPixels } from "../lib/previewDraw";
import { seekPreview, usePreviewOf } from "../store/preview";
import { useTheme } from "../store/settings";
import { useCanvasSize } from "./mix/useCanvasSize";
import { CueFlag } from "./ui/CueFlag";
import { useOverview } from "./useOverview";

export function TrackWaveform({
  trackId,
  durationMs,
  hotCues,
}: {
  trackId: number | null;
  durationMs: number | null;
  hotCues: readonly HotCueMark[];
}) {
  const columns = useOverview(trackId);
  const preview = usePreviewOf(trackId);
  const length = preview?.durationMs ?? durationMs ?? 0;
  const theme = useTheme();
  const [box, setBox] = useState<HTMLDivElement | null>(null);
  const size = useCanvasSize(box);
  const canvas = useRef<HTMLCanvasElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `theme` re-reads the colours.
  const palette = useMemo(() => {
    if (box === null) return null;
    const css = getComputedStyle(box);
    return previewPalette((name) => css.getPropertyValue(name).trim());
  }, [box, theme]);

  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (el == null || ctx == null || size === null || palette === null) return;
    el.width = size.width;
    el.height = size.height;
    if (columns == null || size.width === 0 || size.height === 0) return;
    const pixels = previewPixels(columns, size.width, size.height, palette);
    ctx.putImageData(new ImageData(pixels, size.width, size.height), 0, 0);
  }, [columns, size, palette]);

  function onMouseDown(e: MouseEvent<HTMLDivElement>) {
    if (e.button !== 0 || trackId === null || length <= 0) return;
    // Seeking, not selecting or dragging the row.
    e.stopPropagation();
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    const at = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    void seekPreview(trackId, at * length);
  }

  const head = preview !== null && length > 0 ? preview.positionMs / length : null;
  return (
    <div
      className="relative size-full cursor-pointer py-2.5"
      aria-hidden="true"
      onMouseDown={onMouseDown}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <div ref={setBox} className="size-full">
        <canvas ref={canvas} className="block size-full" />
      </div>
      {columns != null &&
        length > 0 &&
        hotCues.map((cue) => (
          <CueFlag
            key={cue.slot}
            label={hotCueLabel(cue.slot)}
            color={cue.colorRgb === null ? undefined : formatRgb(cue.colorRgb)}
            left={`${Math.min(1, cue.positionMs / length) * 100}%`}
          />
        ))}
      {head !== null && (
        <div
          className="pointer-events-none absolute inset-y-0 w-0.5 bg-accent"
          style={{ left: `${Math.min(1, head) * 100}%` }}
        />
      )}
    </div>
  );
}
