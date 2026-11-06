/**
 * Animates an auto-sized element between sizes when its content or size
 * class changes, so a dialog that grows, shrinks or widens (and stays
 * centred) never jumps. The element keeps its CSS size; on each resize the
 * old height is animated (the width changes at once) to the new ones with the Web Animations
 * API, started before the frame is painted (ResizeObserver runs after layout,
 * before paint). Resizes during an animation are its own frames and are
 * ignored; when it ends, a size that changed meanwhile animates again from
 * where it stopped.
 */
import { useLayoutEffect, useState } from "react";

import { resize } from "../../lib/motion";

interface Size {
  width: number;
  height: number;
}

/** Returns the ref to put on the element (a callback: Radix mounts its content late). */
export function useAnimatedSize(): (el: HTMLElement | null) => void {
  const [el, setEl] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (el === null || typeof ResizeObserver === "undefined" || !("animate" in el)) return;
    const measure = (): Size => {
      const r = el.getBoundingClientRect();
      return { width: r.width, height: r.height };
    };
    let settled = measure();
    let running: Animation | null = null;

    const observer = new ResizeObserver(() => {
      if (running !== null) return;
      const next = measure();
      if (Math.abs(next.width - settled.width) < 1 && Math.abs(next.height - settled.height) < 1) {
        return;
      }
      const from = settled;
      settled = next;
      if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
      // Height only: animating the width rewraps the text on every frame.
      if (Math.abs(next.height - from.height) < 1) return;
      const { duration, easing } = resize();
      // The old size may be over the new size class's caps; lift them while it animates.
      const saved = [el.style.overflow, el.style.maxHeight];
      el.style.overflow = "hidden";
      el.style.maxHeight = "none";
      running = el.animate([{ height: `${from.height}px` }, { height: `${next.height}px` }], {
        duration,
        easing,
      });
      const done = () => {
        running = null;
        [el.style.overflow, el.style.maxHeight] = saved as [string, string];
      };
      running.onfinish = done;
      running.oncancel = done;
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
      running?.cancel();
    };
  }, [el]);
  return setEl;
}
