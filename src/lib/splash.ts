/**
 * The start-up splash from index.html: kept up while the library opens, then
 * faded out, or turned into an error message if it cannot be opened.
 */

/** Below this, the splash would blink rather than read as a loading screen. */
const MIN_VISIBLE_MS = 900;
/** Mirrors --duration-splash-out, plus a frame of slack. */
const FADE_MS = 260;

function splash(): HTMLElement | null {
  return document.getElementById("splash");
}

/** Fades the splash out over the mounted UI and removes it. Timers only:
 * `requestAnimationFrame` and `transitionend` stall in a hidden window. */
export function dismissSplash(): void {
  const el = splash();
  if (!el) return;
  const wait = Math.max(0, MIN_VISIBLE_MS - performance.now());
  window.setTimeout(() => {
    el.dataset.state = "done";
    window.setTimeout(() => el.remove(), FADE_MS);
  }, wait);
}

/** Stops the sweep and shows why the app could not start. */
export function showStartupError(message: string): void {
  const el = splash();
  const text = el?.querySelector<HTMLElement>(".splash-error");
  if (!el || !text) return;
  el.dataset.state = "error";
  el.setAttribute("aria-label", "reks could not start");
  text.textContent = `The library could not be opened: ${message}`;
  text.hidden = false;
}
