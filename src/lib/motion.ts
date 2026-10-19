/**
 * Motion for the slide-over sheet (Web Animations timings). The values live in
 * `src/styles/tokens.css` (`--duration-slide-*`, `--ease-*`); these defaults
 * mirror them for when no stylesheet is loaded (tests). `tokens.test.ts`
 * keeps the two in step.
 */

export interface Timing {
  duration: number;
  easing: string;
}

/** In: decelerates into place (ease-out quint). */
export const SLIDE_IN_DEFAULT: Timing = { duration: 240, easing: "cubic-bezier(0.22, 1, 0.36, 1)" };
/** Out: shorter, and moves at once (fast start) so it follows the click. */
export const SLIDE_OUT_DEFAULT: Timing = {
  duration: 160,
  easing: "cubic-bezier(0.32, 0.72, 0, 1)",
};

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function read(duration: string, easing: string, fallback: Timing): Timing {
  const ms = Number.parseFloat(token(duration));
  const ease = token(easing);
  return {
    duration: Number.isFinite(ms) ? ms : fallback.duration,
    easing: ease === "" ? fallback.easing : ease,
  };
}

export function slideIn(): Timing {
  return read("--duration-slide-in", "--ease-out-quint", SLIDE_IN_DEFAULT);
}

export function slideOut(): Timing {
  return read("--duration-slide-out", "--ease-snap", SLIDE_OUT_DEFAULT);
}
