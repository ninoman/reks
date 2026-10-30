/**
 * Drag-and-drop payloads (HTML5 drag and drop; the window's native file drop
 * is off in `tauri.conf.json`). Browsers hide `dataTransfer` data until the
 * drop, so `dragover` handlers only look at the types.
 */

/** Tracks dragged from the track table. */
export const TRACKS_MIME = "application/x-reks-tracks";
/** A node dragged inside the playlist tree. */
export const NODE_MIME = "application/x-reks-node";
/** A tag or a tag category dragged inside the My Tags drawer (the id is in the store's ref). */
export const TAG_MIME = "application/x-reks-tag";
export const TAG_CATEGORY_MIME = "application/x-reks-tag-category";
/** A loaded mixer deck dragged onto another (its index). */
export const DECK_MIME = "application/x-reks-deck";

export interface TrackDrag {
  trackIds: number[];
  /** The playlist view the rows came from, if any. */
  playlistId: number | null;
  /** Their entry positions in that playlist (same order as `trackIds`). */
  entryPositions: number[];
}

interface Transfer {
  types: readonly string[];
  getData(type: string): string;
}

export function hasType(dt: Transfer | null, type: string): boolean {
  return dt !== null && Array.from(dt.types).includes(type);
}

export function readTracks(dt: Transfer | null): TrackDrag | null {
  const raw = dt?.getData(TRACKS_MIME);
  if (!raw) return null;
  try {
    const v = JSON.parse(raw) as TrackDrag;
    return Array.isArray(v.trackIds) ? v : null;
  } catch {
    return null;
  }
}

function readInt(dt: Transfer | null, type: string): number | null {
  const raw = dt?.getData(type);
  if (!raw) return null;
  const n = Number(raw);
  return Number.isInteger(n) ? n : null;
}

export function readNode(dt: Transfer | null): number | null {
  return readInt(dt, NODE_MIME);
}

export function readDeck(dt: Transfer | null): number | null {
  return readInt(dt, DECK_MIME);
}

/** Where a pointer is on a row: top quarter, bottom quarter or the middle. */
export type DropZone = "before" | "after" | "into";

export function zoneAt(offsetY: number, height: number, allowInto: boolean): DropZone {
  const f = height > 0 ? offsetY / height : 0;
  if (allowInto) return f < 0.25 ? "before" : f > 0.75 ? "after" : "into";
  return f < 0.5 ? "before" : "after";
}
