/** Display helpers for table cells. Presentation only. */

import type { Key, KeyInfo, KeyNotation } from "./ipc";

export function formatBpm(bpmX100: number | null): string {
  return bpmX100 === null ? "" : (bpmX100 / 100).toFixed(2);
}

export function formatDuration(ms: number | null): string {
  if (ms === null) return "";
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function formatRating(rating: number): string {
  return "★".repeat(rating);
}

/** Cue / beat position: `m:ss.mmm`. */
export function formatPosition(ms: number): string {
  const total = Math.round(ms);
  const m = Math.floor(total / 60_000);
  const s = Math.floor((total % 60_000) / 1000);
  return `${m}:${String(s).padStart(2, "0")}.${String(total % 1000).padStart(3, "0")}`;
}

/** Hot-cue slot 0..7 → `A`..`H`. */
export function hotCueLabel(slot: number): string {
  return String.fromCharCode(65 + slot);
}

/** One BPM, or `min–max` for a grid whose tempo changes. */
export function formatBpmRange(minX100: number, maxX100: number): string {
  return minX100 === maxX100 ? formatBpm(minX100) : `${formatBpm(minX100)}–${formatBpm(maxX100)}`;
}

/** 0xRRGGBB → `#rrggbb`. */
export function formatRgb(rgb: number): string {
  return `#${rgb.toString(16).padStart(6, "0")}`;
}

/** A canonical key in the chosen notation, looked up in the facets' labels. */
export function keyLabel(key: Key, notation: KeyNotation, keys: KeyInfo[] | undefined): string {
  if (notation === "classical") return key;
  const k = keys?.find((i) => i.key === key);
  if (k === undefined) return key;
  return notation === "camelot" ? k.camelot : k.openKey;
}

/** Size in decimal units, as Finder and Explorer's drive labels show it: "18.4 GB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return i === 0 ? `${v} B` : `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}
