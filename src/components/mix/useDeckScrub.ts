/**
 * Drag to skim a deck: pointer handlers for an element around the deck's
 * waveforms. On the overview the playhead follows the pointer (a press
 * jumps there); on the zoomed view the track moves with the pointer. A
 * playing deck pauses while dragged and plays on from where it is let go.
 * Seeks go out at most once per `SCRUB_INTERVAL_MS`, the last one always.
 */
import { type PointerEvent, useEffect, useRef } from "react";

import { SCRUB_INTERVAL_MS, SCRUB_SLOP_PX, type ScrubGrip, scrubTarget } from "../../lib/mixView";

export interface ScrubOptions {
  positionMs: number;
  durationMs: number;
  playing: boolean;
  /** Seconds across the zoomed view. */
  viewS: number;
  seek: (ms: number) => void;
  play: () => void;
  pause: () => void;
}

interface Drag {
  pointerId: number;
  grip: ScrubGrip;
  startX: number;
  left: number;
  /** Moved past the slop: a drag, not a press. */
  dragging: boolean;
  /** Paused for the drag; plays again on release. */
  resume: boolean;
}

export function useDeckScrub(opts: ScrubOptions) {
  const live = useRef(opts);
  live.current = opts;
  const drag = useRef<Drag | null>(null);
  const pending = useRef<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => () => clearTimeout(timer.current), []);

  function flush() {
    const ms = pending.current;
    if (ms === null) return;
    pending.current = null;
    live.current.seek(ms);
    timer.current = setTimeout(() => {
      timer.current = undefined;
      flush();
    }, SCRUB_INTERVAL_MS);
  }

  function queue(ms: number) {
    pending.current = ms;
    if (timer.current === undefined) flush();
  }

  function onPointerDown(e: PointerEvent<HTMLElement>) {
    if (e.button !== 0 || drag.current !== null) return;
    const canvas = (e.target as Element).closest("canvas");
    if (canvas === null) return;
    const rect = canvas.getBoundingClientRect();
    const { positionMs, durationMs } = live.current;
    // The overview is the deck's position slider.
    const overview = canvas.getAttribute("role") === "slider";
    const grip: ScrubGrip = overview
      ? { kind: "overview", widthPx: rect.width, durationMs }
      : { kind: "zoomed", pxPerS: rect.width / live.current.viewS, fromMs: positionMs, durationMs };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = {
      pointerId: e.pointerId,
      grip,
      startX: e.clientX,
      left: rect.left,
      dragging: false,
      resume: false,
    };
    if (overview) queue(scrubTarget(grip, e.clientX - rect.left, 0));
  }

  function onPointerMove(e: PointerEvent<HTMLElement>) {
    const d = drag.current;
    if (d === null || d.pointerId !== e.pointerId) return;
    const dx = e.clientX - d.startX;
    if (!d.dragging) {
      if (Math.abs(dx) < SCRUB_SLOP_PX) return;
      d.dragging = true;
      if (live.current.playing) {
        d.resume = true;
        live.current.pause();
      }
    }
    queue(scrubTarget(d.grip, e.clientX - d.left, dx));
  }

  function onPointerEnd(e: PointerEvent<HTMLElement>) {
    const d = drag.current;
    if (d === null || d.pointerId !== e.pointerId) return;
    drag.current = null;
    clearTimeout(timer.current);
    timer.current = undefined;
    if (pending.current !== null) live.current.seek(pending.current);
    pending.current = null;
    if (d.resume) live.current.play();
  }

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp: onPointerEnd,
    onPointerCancel: onPointerEnd,
    onLostPointerCapture: onPointerEnd,
    // Pointer seeks are handled here; the waveform's own click would seek
    // twice. Other clicks (the zoom buttons) go through.
    onClickCapture: (e: { target: EventTarget; stopPropagation: () => void }) => {
      if (e.target instanceof HTMLCanvasElement) e.stopPropagation();
    },
  };
}
