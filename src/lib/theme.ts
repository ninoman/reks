/**
 * The colour theme on <html>: `data-theme` picks a block in tokens.css
 * (Graphite, whose tokens are the plain `:root` ones, needs none). The setting lives in Rust; a copy in
 * localStorage lets the next launch paint the splash in the saved theme
 * before the settings load.
 */
import type { Theme } from "./ipc";

const STORAGE_KEY = "reks.theme";

/** The theme before any is saved; matches `Theme::default()` in Rust. */
export const DEFAULT_THEME: Theme = "booth";

export const THEMES: readonly Theme[] = ["graphite", "paper", "midnight", "dusk", "booth"];

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage off: the next launch starts in Booth until the settings load.
  }
}

/** Puts the last launch's theme on <html> (main.tsx, before anything renders). */
export function applyLastTheme() {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
  } catch {
    // Storage off: fall through to the default.
  }
  document.documentElement.dataset.theme = THEMES.find((t) => t === saved) ?? DEFAULT_THEME;
}
