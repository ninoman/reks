/**
 * A panel that slides in from the right over its positioned parent, so the
 * content under it (the track table) keeps its width. Children mount when it
 * opens and unmount once the slide-out finishes, so a closed panel costs
 * nothing (no IPC).
 *
 * Driven by the Web Animations API rather than a CSS transition: it always
 * runs (no need for the closed position to be painted first), and a toggle
 * mid-slide reverses from wherever the panel is.
 */
import { type ReactNode, useLayoutEffect, useRef, useState } from "react";

import { SLIDE_IN, SLIDE_OUT } from "../lib/motion";

const OFF = "translateX(100%)";

function reducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

export function SlideOver({ open, children }: { open: boolean; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  /** Still on screen: true from opening until the slide-out ends. */
  const [mounted, setMounted] = useState(false);

  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    const running = el.getAnimations?.() ?? [];
    // Mid-slide: carry on from the current position, not from an end.
    const from = running.length > 0 ? getComputedStyle(el).transform : open ? OFF : "none";
    for (const a of running) a.cancel();
    const scale = reducedMotion() ? 0 : 1;

    if (open) {
      setMounted(true);
      el.animate?.([{ transform: from }, { transform: "none" }], {
        ...SLIDE_IN,
        duration: SLIDE_IN.duration * scale,
      });
      return;
    }
    if (typeof el.animate !== "function") {
      setMounted(false);
      return;
    }
    const out = el.animate([{ transform: from }, { transform: OFF }], {
      ...SLIDE_OUT,
      duration: SLIDE_OUT.duration * scale,
      fill: "forwards",
    });
    out.onfinish = () => setMounted(false);
  }, [open]);

  if (!open && !mounted) return null;
  return (
    <div ref={ref} className="slide-over" inert={!open}>
      {children}
    </div>
  );
}
