/**
 * A deck's waveforms on the dark deck: the scrolling view (playhead in the
 * middle, beat lines, cue, memory cue, hot cue and pending loop IN marks, the active loop;
 * Cmd/Ctrl + scroll zooms; `overlay` sits on top of it: the zoom buttons,
 * sync phase and warnings) and the whole track overview under it (click or
 * arrow keys to seek). Both draw in the waveform style chosen in Settings →
 * Decks. Hovering a mark names it (native title: hot cue letter and name,
 * memory cue name).
 *
 * Both canvases draw in device pixels from one animation-frame loop, which
 * runs the playhead on between engine states (`deckScroll.ts`) and copies
 * pre-drawn waveform tiles (`waveStrip.ts`); React renders only feed it.
 */
import {
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { activeLoop, deckMarks, markAt, markTitle } from "../../lib/deckMarks";
import {
  followPlayhead,
  frameMs,
  type PlayheadSample,
  predict,
  scrollFrame,
  zoomSeconds,
} from "../../lib/deckScroll";
import type { BeatMark, HotCueView, LoopView, MemoryCueView } from "../../lib/ipc";
import { WaveStrip } from "../../lib/waveStrip";
import { useSettings, useTheme } from "../../store/settings";
import { focusRingInset } from "../ui/control";
import { drawOverview, drawScroll, type Inputs, readTokens } from "./deckCanvas";
import { useCanvasSize } from "./useCanvasSize";

/** Arrow-key seek step on the overview, ms. */
const SEEK_STEP_MS = 2_000;
/** How near (CSS px) the pointer must be to a mark to name it. */
const MARK_HOVER_PX = 4;
/** Scroll travel (px) per zoom step with Cmd/Ctrl held; a trackpad sends many small deltas. */
const WHEEL_STEP_PX = 40;

export interface DeckWaveformProps {
  deck: number;
  columns: Uint8Array | null;
  beats: BeatMark[] | null;
  positionMs: number;
  durationMs: number;
  playing: boolean;
  /** Playback speed ratio while playing. */
  speed: number;
  cueMs: number;
  hotCues: HotCueView[];
  memoryCues: MemoryCueView[];
  /** The active loop, if any. */
  loop: LoopView | null;
  /** A pending loop IN (ms), waiting for OUT. */
  loopInMs: number | null;
  /** `DECK_ZOOM_S` index. */
  zoom: number;
  onSeek: (ms: number) => void;
  /** 1 zooms in, -1 out. */
  onZoom: (dir: -1 | 1) => void;
  /** Drawn over the scrolling view. */
  overlay?: ReactNode;
}

export function DeckWaveform({
  deck,
  columns,
  beats,
  positionMs,
  durationMs,
  playing,
  speed,
  cueMs,
  hotCues,
  memoryCues,
  loop,
  loopInMs,
  zoom,
  onSeek,
  onZoom,
  overlay,
}: DeckWaveformProps) {
  const style = useSettings((s) => s.settings?.deckWaveform ?? "rgb");
  const [scrollBox, setScrollBox] = useState<HTMLDivElement | null>(null);
  const [overviewBox, setOverviewBox] = useState<HTMLDivElement | null>(null);
  const scroll = useRef<HTMLCanvasElement>(null);
  const overview = useRef<HTMLCanvasElement>(null);
  const scrollSize = useCanvasSize(scrollBox);
  const overviewSize = useCanvasSize(overviewBox);
  const seconds = zoomSeconds(zoom);

  const theme = useTheme();
  // biome-ignore lint/correctness/useExhaustiveDependencies: `theme` re-reads the deck and waveform colours.
  const tokens = useMemo(
    () => (scrollBox === null ? null : readTokens(scrollBox)),
    [scrollBox, theme],
  );
  const strip = useMemo(
    () =>
      columns === null || tokens === null || scrollSize === null || scrollSize.width === 0
        ? null
        : new WaveStrip({
            columns,
            style,
            palette: tokens.palette,
            pxPerS: scrollSize.width / seconds,
            height: scrollSize.height,
          }),
    [columns, style, tokens, scrollSize, seconds],
  );
  const overviewStrip = useMemo(
    () =>
      columns === null || tokens === null || overviewSize === null || durationMs <= 0
        ? null
        : new WaveStrip({
            columns,
            style,
            palette: tokens.palette,
            pxPerS: (overviewSize.width * 1000) / durationMs,
            height: overviewSize.height,
          }),
    [columns, style, tokens, overviewSize, durationMs],
  );
  const marks = useMemo(
    () => (tokens === null ? [] : deckMarks(hotCues, memoryCues, cueMs, loopInMs, tokens.marks)),
    [hotCues, memoryCues, cueMs, loopInMs, tokens],
  );

  const loopStart = activeLoop(loop)?.startMs ?? null;
  const loopEnd = activeLoop(loop)?.endMs ?? null;
  const inputs = useRef<Inputs | null>(null);
  // Bumped when anything but the playhead changes, so a still deck redraws.
  const version = useRef(0);
  useLayoutEffect(() => {
    inputs.current = {
      scrollSize,
      overviewSize,
      strip,
      overviewStrip,
      tokens,
      beats,
      marks,
      loop: loopStart === null || loopEnd === null ? null : { startMs: loopStart, endMs: loopEnd },
      seconds,
      durationMs,
    };
    version.current++;
  }, [
    scrollSize,
    overviewSize,
    strip,
    overviewStrip,
    tokens,
    beats,
    marks,
    loopStart,
    loopEnd,
    seconds,
    durationMs,
  ]);

  const sample = useRef<PlayheadSample>({ positionMs, speed: 0, at: 0 });
  useLayoutEffect(() => {
    sample.current = { positionMs, speed: playing ? speed : 0, at: performance.now() };
  }, [positionMs, playing, speed]);
  /** The playhead as drawn, ms. */
  const shown = useRef<number | null>(null);

  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let drawn = "";
    function frame(now: number) {
      raf = requestAnimationFrame(frame);
      const s = sample.current;
      const ms = followPlayhead(shown.current, now - last, s.speed, predict(s, now));
      last = now;
      const i = inputs.current;
      shown.current = Math.min(Math.max(0, ms), i?.durationMs ?? ms);
      if (i === null || i.tokens === null) return;
      const at = shown.current;
      // Redraw only when a pixel would change.
      const scrollLeft =
        i.scrollSize === null ? 0 : scrollFrame(at, i.seconds, i.scrollSize.width).left;
      const overviewX =
        i.overviewSize === null || i.durationMs <= 0
          ? 0
          : Math.round((at / i.durationMs) * i.overviewSize.width);
      const key = `${version.current}:${scrollLeft}:${overviewX}`;
      if (key === drawn) {
        if (i.scrollSize !== null) i.strip?.prefetch(scrollLeft, i.scrollSize.width);
        return;
      }
      drawn = key;
      if (scroll.current !== null && i.scrollSize !== null)
        drawScroll(scroll.current, i.scrollSize, i, i.tokens, at);
      if (i.scrollSize !== null) i.strip?.prefetch(scrollLeft, i.scrollSize.width);
      if (overview.current !== null && i.overviewSize !== null)
        drawOverview(overview.current, i.overviewSize, i, i.tokens, at);
    }
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);

  // Cmd/Ctrl + scroll zooms; a listener of our own as React's wheel is passive.
  const zoomRef = useRef(onZoom);
  zoomRef.current = onZoom;
  useEffect(() => {
    if (scrollBox === null) return;
    let travel = 0;
    function onWheel(e: WheelEvent) {
      if (!(e.metaKey || e.ctrlKey)) return;
      e.preventDefault();
      travel += e.deltaY;
      if (Math.abs(travel) < WHEEL_STEP_PX) return;
      zoomRef.current(travel < 0 ? 1 : -1);
      travel = 0;
    }
    scrollBox.addEventListener("wheel", onWheel, { passive: false });
    return () => scrollBox.removeEventListener("wheel", onWheel);
  }, [scrollBox]);

  function seekScroll(e: MouseEvent<HTMLCanvasElement>) {
    if (scrollSize === null || scrollSize.width === 0) return;
    const x = (e.clientX - e.currentTarget.getBoundingClientRect().left) * scrollSize.dpr;
    const f = scrollFrame(shown.current ?? positionMs, seconds, scrollSize.width);
    onSeek(Math.min(durationMs, Math.max(0, frameMs(f, x))));
  }

  function seekOverview(e: MouseEvent<HTMLCanvasElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) return;
    onSeek(((e.clientX - rect.left) / rect.width) * durationMs);
  }

  /** Names the mark under the pointer at `ms`, `msPerPx` per CSS px, in the canvas title. */
  function hoverMark(canvas: HTMLCanvasElement, ms: number, msPerPx: number) {
    const m = markAt(marks, ms, MARK_HOVER_PX * msPerPx);
    const title = m === null ? "" : markTitle(m);
    if (canvas.title !== title) canvas.title = title;
  }

  function hoverScroll(e: MouseEvent<HTMLCanvasElement>) {
    if (scrollSize === null || scrollSize.width === 0) return;
    const x = (e.clientX - e.currentTarget.getBoundingClientRect().left) * scrollSize.dpr;
    const f = scrollFrame(shown.current ?? positionMs, seconds, scrollSize.width);
    const msPerPx = (seconds * 1000 * scrollSize.dpr) / scrollSize.width;
    hoverMark(e.currentTarget, frameMs(f, x), msPerPx);
  }

  function hoverOverview(e: MouseEvent<HTMLCanvasElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) return;
    const ms = ((e.clientX - rect.left) / rect.width) * durationMs;
    hoverMark(e.currentTarget, ms, durationMs / rect.width);
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

  const n = deck + 1;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-1.5">
      <div
        ref={setScrollBox}
        className="group/wave relative min-h-(--deck-wave-min-h) flex-1 overflow-hidden rounded-xs bg-deck"
      >
        {/* Pointer-only: the overview below is the keyboard way to seek. */}
        <canvas
          ref={scroll}
          className="block"
          data-zoom={seconds}
          onClick={seekScroll}
          onMouseMove={hoverScroll}
        />
        {overlay}
      </div>
      <div
        ref={setOverviewBox}
        className="h-(--deck-overview-h) shrink-0 overflow-hidden rounded-xs bg-deck"
      >
        <canvas
          ref={overview}
          role="slider"
          tabIndex={0}
          aria-label={`Deck ${n} position`}
          aria-valuemin={0}
          aria-valuemax={Math.round(durationMs)}
          aria-valuenow={Math.round(positionMs)}
          className={`block ${focusRingInset}`}
          onClick={seekOverview}
          onMouseMove={hoverOverview}
          onKeyDown={overviewKey}
        />
      </div>
    </div>
  );
}
