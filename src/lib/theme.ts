/**
 * The colour theme on <html>: `data-theme` picks a block in tokens.css
 * (Graphite, the default, needs none). The setting lives in Rust; a copy in
 * localStorage lets the next launch paint the splash in the saved theme
 * before the settings load.
 */
import type { Theme } from "./ipc";

const STORAGE_KEY = "reks.theme";

export const THEMES: readonly Theme[] = ["graphite", "paper", "midnight", "dusk", "booth"];

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage off: the next launch starts in Graphite until the settings load.
  }
}

/** Puts the last launch's theme on <html> (main.tsx, before anything renders). */
export function applyLastTheme() {
  let saved: string | null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
  } catch {
    return;
  }
  const theme = THEMES.find((t) => t === saved);
  if (theme !== undefined) document.documentElement.dataset.theme = theme;
}
