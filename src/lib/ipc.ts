/**
 * Typed wrappers for the Tauri commands (src-tauri/src/commands.rs). The
 * types mirror the Rust serde shapes; `crates/reks-core/tests/query.rs`
 * (`serde_shapes`) pins the query side.
 */
import { Channel, invoke } from "@tauri-apps/api/core";

/** Canonical key name from `keys.rs`, e.g. `Am`, `F#m`, `Db`. */
export type Key = string;

export type Filter =
  | { type: "genre"; genre: string }
  | { type: "keyIn"; keys: Key[] }
  /** BPM × 100, inclusive. */
  | { type: "bpmRange"; min: number; max: number }
  | { type: "color"; color: string }
  | { type: "ratingMin"; rating: number }
  | { type: "playlist"; id: number }
  | { type: "missing"; missing: boolean }
  | { type: "needsAnalysis"; needs: boolean };

export type SortColumn =
  | "title"
  | "artist"
  | "album"
  | "genre"
  | "label"
  | "bpm"
  | "key"
  | "rating"
  | "color"
  | "duration"
  | "year"
  | "dateAdded"
  | "playCount"
  | "kind"
  | "bitRate"
  | "playlistPosition";

export interface Sort {
  column: SortColumn;
  descending: boolean;
}

export interface TrackQuery {
  text: string | null;
  filters: Filter[];
  sort: Sort;
  offset: number;
  limit: number;
}

export interface TrackRow {
  id: number;
  title: string | null;
  artist: string | null;
  album: string | null;
  genre: string | null;
  label: string | null;
  remixer: string | null;
  comment: string | null;
  key: Key | null;
  bpmX100: number | null;
  rating: number;
  color: string | null;
  durationMs: number | null;
  year: number | null;
  kind: string;
  bitRateKbps: number | null;
  dateAdded: string;
  playCount: number;
  missing: boolean;
  needsAnalysis: boolean;
}

export interface TrackPage {
  rows: TrackRow[];
  total: number;
}

export interface KeyInfo {
  key: Key;
  camelot: string;
}

export interface PlaylistNode {
  id: number;
  parentId: number | null;
  name: string;
  isFolder: boolean;
  depth: number;
}

export interface Facets {
  genres: string[];
  keys: KeyInfo[];
  colours: string[];
  playlists: PlaylistNode[];
}

export type ScanEvent =
  | { type: "found"; path: string }
  | { type: "progress"; done: number; total: number }
  | { type: "added"; id: number; path: string }
  | { type: "updated"; id: number; path: string }
  | { type: "moved"; id: number; from: string; to: string }
  | { type: "missing"; id: number; path: string }
  | { type: "error"; path: string; message: string };

export interface ScanReport {
  found: number;
  added: number;
  updated: number;
  unchanged: number;
  moved: number;
  missing: number;
  errors: { path: string; message: string }[];
  cancelled: boolean;
}

export type UnmatchedReason =
  | { kind: "noLocation" }
  | { kind: "notAFile" }
  | { kind: "invalidLocation"; message: string }
  | { kind: "fileNotFound"; path: string }
  | { kind: "unreadable"; path: string; message: string }
  | { kind: "duplicateOf"; trackId: number };

export interface Unmatched {
  trackId: number;
  name: string | null;
  artist: string | null;
  location: string | null;
  reason: UnmatchedReason;
}

export type ImportEvent =
  | { type: "progress"; stage: "files" | "database"; done: number; total: number }
  | ({ type: "unmatched" } & Unmatched);

export interface ImportReport {
  total: number;
  matchedByPath: number;
  matchedByLocation: number;
  matchedByFallback: number;
  added: number;
  missing: number;
  unmatched: Unmatched[];
  warnings: { trackId: number | null; message: string }[];
  beatgridRows: number;
  cueRows: number;
  playlistsCreated: number;
  playlistsMatched: number;
  playlistEntries: number;
  playlistEntriesSkipped: number;
  elapsedMs: number;
}

/** Replace a path prefix in every XML `Location` (music folder moved). */
export interface PrefixRemap {
  from: string;
  to: string;
}

export function libraryQuery(query: TrackQuery): Promise<TrackPage> {
  return invoke("library_query", { query });
}

export function libraryFacets(): Promise<Facets> {
  return invoke("library_facets");
}

export function scanStart(roots: string[], onEvent: (e: ScanEvent) => void): Promise<ScanReport> {
  const channel = new Channel<ScanEvent>();
  channel.onmessage = onEvent;
  return invoke("scan_start", { roots, onEvent: channel });
}

export function scanCancel(): Promise<void> {
  return invoke("scan_cancel");
}

export function rbxmlImportStart(
  path: string,
  remap: PrefixRemap[],
  onEvent: (e: ImportEvent) => void,
): Promise<ImportReport> {
  const channel = new Channel<ImportEvent>();
  channel.onmessage = onEvent;
  return invoke("rbxml_import_start", { path, remap, onEvent: channel });
}
