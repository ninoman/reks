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
  /** Position of this entry in a playlist view (one row per entry); null elsewhere. */
  entryPosition: number | null;
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
  /** Index among its siblings. */
  position: number;
  /** Number of entries (0 for folders). */
  entries: number;
}

/** Depth first, siblings in order. */
export interface PlaylistTree {
  nodes: PlaylistNode[];
}

/** What "add tracks" does with tracks already in the playlist. */
export type DuplicateMode = "ask" | "allow" | "skip";

/**
 * A playlist edit (`playlist.rs`). `position` / `before` are indices in the
 * list as it is now (its length = the end); reks-core does the rest.
 */
export type PlaylistOp =
  | { type: "createPlaylist"; parentId: number | null; name: string; position?: number | null }
  | { type: "createFolder"; parentId: number | null; name: string; position?: number | null }
  | { type: "rename"; id: number; name: string }
  | { type: "delete"; id: number }
  | { type: "move"; id: number; parentId: number | null; position: number }
  | {
      type: "addTracks";
      playlistId: number;
      trackIds: number[];
      before?: number | null;
      duplicates: DuplicateMode;
    }
  | { type: "removeEntries"; playlistId: number; positions: number[] }
  | { type: "reorderEntries"; playlistId: number; positions: number[]; before: number };

export type PlaylistOutcome =
  | { type: "done"; createdId: number | null; added: number; skipped: number }
  /** "Add tracks" with `ask` found duplicates; nothing was written. */
  | {
      type: "duplicates";
      playlistId: number;
      /** The duplicated tracks, each once. */
      trackIds: number[];
      duplicateCount: number;
      requested: number;
    };

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
  /** Deepest folder holding every file not found; the remap `from` to suggest. */
  missingRoot: string | null;
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

export interface ArtworkInfo {
  id: number;
  mime: string;
  width: number | null;
  height: number | null;
}

/** Every track column the detail panel shows; a superset of `TrackRow`. */
export interface Track extends TrackRow {
  path: string;
  fileSize: number;
  fileMtime: string;
  albumArtist: string | null;
  composer: string | null;
  mix: string | null;
  grouping: string | null;
  trackNo: number | null;
  discNo: number | null;
  sampleRate: number | null;
  bitDepth: number | null;
  channels: number | null;
  keySource: string | null;
  bpmSource: string | null;
  lastPlayed: string | null;
  rbTrackId: number | null;
  createdAt: string;
  updatedAt: string;
  artwork: ArtworkInfo | null;
}

/** Hot cue (`slot` 0..7 = A..H), memory cue (`slot` null) or loop (`endMs` set). */
export interface Cue {
  id: number;
  slot: number | null;
  /** rekordbox Type: 0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop. */
  kind: number;
  startMs: number;
  endMs: number | null;
  name: string | null;
  /** 0xRRGGBB. */
  colorRgb: number | null;
}

export interface BeatgridSummary {
  markers: number;
  firstBeatMs: number;
  bpmMinX100: number;
  bpmMaxX100: number;
}

export interface TrackDetail {
  track: Track;
  cues: Cue[];
  /** null = no grid (needs analysis). */
  beatgrid: BeatgridSummary | null;
}

/**
 * A metadata edit: an absent field is unchanged, `null` (or empty text)
 * clears it. Validated in `reks-core` (`track.rs`).
 */
export interface TrackPatch {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  albumArtist?: string | null;
  genre?: string | null;
  composer?: string | null;
  remixer?: string | null;
  label?: string | null;
  mix?: string | null;
  grouping?: string | null;
  comment?: string | null;
  trackNo?: number | null;
  discNo?: number | null;
  year?: number | null;
  rating?: number;
  color?: string | null;
  /** Any notation; stored canonical. */
  key?: string | null;
}

export function libraryTrack(id: number): Promise<TrackDetail> {
  return invoke("library_track", { id });
}

export function libraryUpdateTrack(id: number, patch: TrackPatch): Promise<TrackDetail> {
  return invoke("library_update_track", { id, patch });
}

/** Artwork image bytes (raw `ipc::Response`, not JSON). */
export function artworkGet(id: number): Promise<ArrayBuffer> {
  return invoke("artwork_get", { id });
}

export function playlistsGet(): Promise<PlaylistTree> {
  return invoke("playlists_get");
}

export function playlistsMutate(op: PlaylistOp): Promise<PlaylistOutcome> {
  return invoke("playlists_mutate", { op });
}

/** `player_*` commands: the preview player (reks-mix, docs/decisions.md D5). */
export interface PlayerStatus {
  loaded: boolean;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  /** 0..1, the knob position (squared taper in Rust). */
  volume: number;
  device: string | null;
  deviceLost: boolean;
  trackId: number | null;
}

/** Output device names, the system default first. */
export function playerDevices(): Promise<string[]> {
  return invoke("player_devices");
}

export function playerLoad(id: number, play: boolean): Promise<PlayerStatus> {
  return invoke("player_load", { id, play });
}

/** `null`: the system default. */
export function playerSetDevice(device: string | null): Promise<PlayerStatus> {
  return invoke("player_set_device", { device });
}

export function playerPlay(): Promise<PlayerStatus> {
  return invoke("player_play");
}

export function playerPause(): Promise<PlayerStatus> {
  return invoke("player_pause");
}

export function playerSeek(ms: number): Promise<PlayerStatus> {
  return invoke("player_seek", { ms: Math.max(0, Math.round(ms)) });
}

export function playerVolume(volume: number): Promise<PlayerStatus> {
  return invoke("player_volume", { volume });
}

export function playerState(): Promise<PlayerStatus> {
  return invoke("player_state");
}

/**
 * Overview waveform: little-endian f32 `[min, max]` pairs, one per 1,000
 * source frames (raw `ipc::Response`, not JSON).
 */
export function overviewWaveform(id: number): Promise<ArrayBuffer> {
  return invoke("overview_waveform", { id });
}
