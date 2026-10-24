/**
 * The deck waveform zoom the user last picked, kept in localStorage so a
 * relaunch opens at the same zoom. A view preference, so it stays out of the
 * Rust settings.
 */
import { DECK_ZOOM_S, DEFAULT_DECK_ZOOM } from "./deckScroll";

const STORAGE_KEY = "reks.deckZoom";

export function loadDeckZoom(): number {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
  } catch {
    // Storage off: fall through to the default.
  }
  const zoom = Number(saved);
  return saved !== null && Number.isInteger(zoom) && zoom >= 0 && zoom < DECK_ZOOM_S.length
    ? zoom
    : DEFAULT_DECK_ZOOM;
}

export function saveDeckZoom(zoom: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(zoom));
  } catch {
    // Storage off: the next launch opens at the default zoom.
  }
}
