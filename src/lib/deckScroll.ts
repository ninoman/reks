/**
 * The scrolling deck waveform's geometry (pure): zoom levels, where the view
 * starts in device pixels, and a playhead that moves every frame between the
 * engine's ~60 Hz states. Everything is placed on one integer device-pixel
 * grid fixed to the track, so the waveform, beat lines and markers step
 * together and a column never changes pixel as the view scrolls.
 */

/** Seconds across the view, widest first; zoom is an index into this. */
export const DECK_ZOOM_S = [32, 16, 8, 4, 2, 1, 0.5] as const;
export const DEFAULT_DECK_ZOOM = 2;

/** The next zoom index in `dir` (1 = in, closer), clamped to the ends. */
export function stepZoom(zoom: number, dir: -1 | 1): number {
  return Math.min(DECK_ZOOM_S.length - 1, Math.max(0, zoom + dir));
}

export function zoomSeconds(zoom: number): number {
  return DECK_ZOOM_S[zoom] ?? DECK_ZOOM_S[DEFAULT_DECK_ZOOM];
}

/** One frame of the scrolling view, in device pixels. */
export interface ScrollFrame {
  /** Device pixels per second of track. */
  pxPerS: number;
  /** Track pixel at the view's left edge (the playhead sits at `left + mid`). */
  left: number;
  /** The playhead's x in the view. */
  mid: number;
}

/** The view `width` device px wide showing `seconds` around `positionMs`. */
export function scrollFrame(positionMs: number, seconds: number, width: number): ScrollFrame {
  const pxPerS = width / seconds;
  const mid = Math.floor(width / 2);
  return { pxPerS, left: Math.round((positionMs / 1000) * pxPerS) - mid, mid };
}

/** x of `ms` in a frame, on the same pixel grid as the waveform. */
export function frameX(f: ScrollFrame, ms: number): number {
  return Math.round((ms / 1000) * f.pxPerS) - f.left;
}

/** ms at view x (device px); the inverse of [`frameX`] up to rounding. */
export function frameMs(f: ScrollFrame, x: number): number {
  return ((f.left + x) / f.pxPerS) * 1000;
}

/** Further than this from the engine's position, the playhead jumps (a seek). */
const SNAP_MS = 40;
/** Share of the gap to the engine's position closed per frame. */
const PULL = 0.15;
/** Never run on more than this past the last engine state (stalled output). */
export const MAX_AHEAD_MS = 100;

/** The latest engine state of a deck, with when it arrived (`performance.now()`). */
export interface PlayheadSample {
  positionMs: number;
  /** Playback speed ratio; 0 when stopped. */
  speed: number;
  at: number;
}

/** Where the engine's playhead should be at `now`. */
export function predict(s: PlayheadSample, now: number): number {
  return s.positionMs + Math.min(MAX_AHEAD_MS, Math.max(0, now - s.at)) * s.speed;
}

/**
 * The drawn playhead after `dtMs` more: it runs on at `speed` and is pulled
 * gently towards the engine's `target` (state arrival jitters by a few ms,
 * which would show as uneven steps), or jumps there after a seek. A stopped
 * deck (being skimmed, say) shows the engine's position as it is.
 */
export function followPlayhead(
  shown: number | null,
  dtMs: number,
  speed: number,
  target: number,
): number {
  if (shown === null || speed === 0) return target;
  const ran = shown + dtMs * speed;
  const gap = target - ran;
  return Math.abs(gap) > SNAP_MS ? target : ran + gap * PULL;
}
