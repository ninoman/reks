/**
 * A deck's waveforms on the dark deck: the scrolling view (playhead in the
 * middle, beat lines, cue and hot cue markers; click to seek) and the whole
 * track overview under it (click or arrow keys to seek). Both draw in the
 * waveform style chosen in Settings → Decks.
 */
import { type KeyboardEvent, type MouseEvent, useEffect, useState } from "react";

import type { BeatMark, HotCueView } from "../../lib/ipc";
import {
  drawBeats,
  drawMarker,
  drawWave,
  prepare,
  type WaveView,
  wavePalette,
} from "../../lib/waveDraw";
import { useSettings } from "../../store/settings";
import { focusRingInset } from "../ui/control";

/** Seconds shown across the scrolling view. */
export const VIEW_S = 8;
/** Arrow-key seek step on the overview, ms. */
const SEEK_STEP_MS = 2_000;

export interface DeckWaveformProps {
  deck: number;
  columns: Uint8Array | null;
  beats: BeatMark[] | null;
  positionMs: number;
  durationMs: number;
  cueMs: number;
  hotCues: HotCueView[];
  compact: boolean;
  onSeek: (ms: number) => void;
}

function useWidth(el: HTMLElement | null): number {
  const [width, setWidth] = useState(0);
  useEffect(() => {
    if (el === null) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.floor(entry?.contentRect.width ?? 0)),
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [el]);
  return width;
}

export function DeckWaveform({
  deck,
  columns,
  beats,
  positionMs,
  durationMs,
  cueMs,
  hotCues,
  compact,
  onSeek,
}: DeckWaveformProps) {
  const style = useSettings((s) => s.settings?.deckWaveform ?? "rgb");
  const [scroll, setScroll] = useState<HTMLCanvasElement | null>(null);
  const [overview, setOverview] = useState<HTMLCanvasElement | null>(null);
  const width = useWidth(scroll);
  const pxPerS = width / VIEW_S;
  const startMs = positionMs - (VIEW_S * 1000) / 2;

  useEffect(() => {
    if (scroll === null || width === 0) return;
    const height = scroll.clientHeight;
    const p = prepare(scroll, width, height);
    if (p === null) return;
    const v: WaveView = { startMs, pxPerS, width, height };
    if (columns !== null) drawWave(p.ctx, wavePalette(p.token), style, columns, v);
    if (beats !== null) drawBeats(p.ctx, p.token, beats, v);
    for (const c of hotCues) drawMarker(p.ctx, p.token("--color-deck-hotcue"), c.positionMs, v, 2);
    drawMarker(p.ctx, p.token("--color-deck-cue"), cueMs, v, 2);
    drawMarker(p.ctx, p.token("--color-deck-fg"), positionMs, v, 2);
  }, [scroll, width, style, columns, beats, hotCues, cueMs, positionMs, startMs, pxPerS]);

  useEffect(() => {
    if (overview === null || width === 0 || durationMs <= 0) return;
    const height = overview.clientHeight;
    const p = prepare(overview, width, height);
    if (p === null) return;
    const v: WaveView = { startMs: 0, pxPerS: (width * 1000) / durationMs, width, height };
    if (columns !== null) drawWave(p.ctx, wavePalette(p.token), style, columns, v);
    p.ctx.fillStyle = p.token("--color-overlay");
    p.ctx.fillRect(0, 0, Math.round((positionMs / durationMs) * width), height);
    drawMarker(p.ctx, p.token("--color-deck-cue"), cueMs, v);
    drawMarker(p.ctx, p.token("--color-deck-fg"), positionMs, v, 2);
  }, [overview, width, style, columns, durationMs, cueMs, positionMs]);

  function seekScroll(e: MouseEvent<HTMLCanvasElement>) {
    if (pxPerS <= 0) return;
    const x = e.clientX - e.currentTarget.getBoundingClientRect().left;
    onSeek(Math.min(durationMs, Math.max(0, startMs + (x / pxPerS) * 1000)));
  }

  function seekOverview(e: MouseEvent<HTMLCanvasElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) return;
    onSeek(((e.clientX - rect.left) / rect.width) * durationMs);
  }

  function overviewKey(e: KeyboardEvent<HTMLCanvasElement>) {
    const next =
      e.key === "ArrowRight"
        ? positionMs + SEEK_STEP_MS
        : e.key === "ArrowLeft"
          ? positionMs - SEEK_STEP_MS
          : e.key === "Home"
            ? 0
            : null;
    if (next === null) return;
    e.preventDefault();
    onSeek(Math.min(durationMs, Math.max(0, next)));
  }

  return (
    <div className="flex flex-col gap-px bg-deck">
      {/* Pointer-only: the overview below is the keyboard way to seek. */}
      <canvas
        ref={setScroll}
        className={compact ? "h-(--deck-wave-h-compact) w-full" : "h-(--deck-wave-h) w-full"}
        onClick={seekScroll}
      />
      <canvas
        ref={setOverview}
        role="slider"
        tabIndex={0}
        aria-label={`Deck ${deck + 1} position`}
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs)}
        aria-valuenow={Math.round(positionMs)}
        className={`h-(--deck-overview-h) w-full ${focusRingInset}`}
        onClick={seekOverview}
        onKeyDown={overviewKey}
      />
    </div>
  );
}
