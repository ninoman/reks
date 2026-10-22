/**
 * Typed wrappers for the Tauri commands (src-tauri/src/commands.rs). The
 * types mirror the Rust serde shapes; `crates/reks-core/tests/query.rs`
 * (`serde_shapes`) pins the query side.
 */
import { Channel, invoke } from "@tauri-apps/api/core";

/** Resolves once the library is open (opened off the main thread at start-up);
 * rejects with the reason it could not be. Call before any other command. */
export function appReady(): Promise<void> {
  return invoke("app_ready");
}

/** Row id of a My Tags category or tag (one id space). */
export type TagId = number;

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
  /** Play count 0 and no last played (D23). */
  | { type: "neverPlayed" }
  | { type: "playCountRange"; min: number; max: number }
  /** Last played on or after `date` (`YYYY-MM-DD`). */
  | { type: "playedSince"; date: string }
  /** The track has at least one of these My Tags. */
  | { type: "tagAny"; ids: TagId[] };

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
  | "lastPlayed"
  | "kind"
  | "bitRate"
  | "playlistPosition";

export interface Sort {
  column: SortColumn;
  descending: boolean;
}

/**
 * A rule tree (`rule.rs`). A group matches all or any of its rules; `not`
 * negates it ("none of these" = `match: "any"` with `not`). NOT of a filter
 * keeps tracks where the field is empty. Core limits: depth 8, 200 nodes.
 */
export type RuleNode =
  | { type: "group"; match: "all" | "any"; not?: boolean; rules?: RuleNode[] }
  | { type: "filter"; not?: boolean; filter: Filter };

export interface TrackQuery {
  text: string | null;
  filters: Filter[];
  /** ANDed with `filters` and `text`. */
  rule?: RuleNode;
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
  /** Baseline plus included history sessions (D23). */
  playCount: number;
  /** `YYYY-MM-DD` (or a rekordbox date); derived like `playCount`. */
  lastPlayed: string | null;
  missing: boolean;
  needsAnalysis: boolean;
  /** Position of this entry in a playlist view (one row per entry); null elsewhere. */
  entryPosition: number | null;
  /** My Tags on the track, ordered by category then tag display order. */
  tags: TagId[];
}

/** A divider entry in a playlist (D22); `position` numbers entries like `entryPosition`. */
export interface DividerRow {
  position: number;
  label: string;
  /** Plain ASCII; other labels are flagged until players are tested (DIV2). */
  ascii: boolean;
}

export interface TrackPage {
  rows: TrackRow[];
  /** Tracks over all pages; dividers are not counted. */
  total: number;
  /** Every divider of the playlist, on every page, when `dividerView`; else empty. */
  dividers: DividerRow[];
  /** A playlist in playlist order with no search or other filter: dividers show here. */
  dividerView: boolean;
}

export interface KeyInfo {
  key: Key;
  camelot: string;
  openKey: string;
}

export interface PlaylistNode {
  id: number;
  parentId: number | null;
  name: string;
  isFolder: boolean;
  depth: number;
  /** Index among its siblings. */
  position: number;
  /** Number of entries (0 for folders); for a smart playlist, the tracks its rule matches now. */
  entries: number;
  /** A smart playlist (D29): its tracks follow a saved rule. */
  smart: boolean;
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
  | { type: "reorderEntries"; playlistId: number; positions: number[]; before: number }
  /** Before the entry at `position`; absent or null = appended. Label rules in reks-core. */
  | { type: "insertDivider"; playlistId: number; position?: number | null; label: string }
  | { type: "renameDivider"; playlistId: number; position: number; label: string }
  /** A smart playlist; `sort` absent = newest first (never `playlistPosition`). */
  | {
      type: "createSmart";
      parentId?: number | null;
      name: string;
      rule: RuleNode;
      sort?: Sort;
      position?: number | null;
    }
  /** Replaces a smart playlist's rule; `sort` absent keeps its sort. */
  | { type: "setRule"; id: number; rule: RuleNode; sort?: Sort };

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
  tagsCreated: number;
  tagsAssigned: number;
  /** Deepest folder holding every file not found; the remap `from` to suggest. */
  missingRoot: string | null;
  elapsedMs: number;
  /** Stopped while files were read: nothing was written. */
  cancelled: boolean;
}

/** What an import would find, before anything is written (`rbxml::preview`). */
export interface ImportPreview {
  total: number;
  playlists: number;
  folders: number;
  playlistEntries: number;
  withGrid: number;
  withCues: number;
  filesFound: number;
  filesNotFound: number;
  noLocation: number;
  notAFile: number;
  invalidLocation: number;
  /** Already in the library by path or earlier Location (an estimate). */
  inLibrary: number;
  missingRoot: string | null;
  /** The first few paths not found. */
  notFoundSample: string[];
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

/** Background analysis counts (`reks_analysis::QueueStatus`); done/failed/total cover the current batch. */
export interface AnalysisStatus {
  queued: number;
  running: number;
  done: number;
  failed: number;
  total: number;
  paused: boolean;
}

/** Which library fields an analysis changed (manual and rekordbox data never change). */
export interface AnalysisStored {
  grid: boolean;
  bpm: boolean;
  key: boolean;
}

export type AnalysisEvent =
  | { type: "analysed"; trackId: number; cached: boolean; stored: AnalysisStored }
  | { type: "failed"; trackId: number; message: string }
  | { type: "status"; status: AnalysisStatus };

/** Sends every analysis event to `onEvent`; resolves to the current counts, `null` when analysis is off. */
export function analysisSubscribe(
  onEvent: (e: AnalysisEvent) => void,
): Promise<AnalysisStatus | null> {
  const channel = new Channel<AnalysisEvent>();
  channel.onmessage = onEvent;
  return invoke("analysis_subscribe", { onEvent: channel });
}

export function analysisPause(): Promise<AnalysisStatus> {
  return invoke("analysis_pause");
}

export function analysisResume(): Promise<AnalysisStatus> {
  return invoke("analysis_resume");
}

/** Drops queued tracks; the ones running finish but are not stored. */
export function analysisCancel(): Promise<AnalysisStatus> {
  return invoke("analysis_cancel");
}

/** Re-analyses `ids` first, ignoring the cache; resolves to how many were queued. */
export function analysisReanalyse(ids: number[]): Promise<number> {
  return invoke("analysis_reanalyse", { ids });
}

export function rbxmlPreview(path: string, remap: PrefixRemap[]): Promise<ImportPreview> {
  return invoke("rbxml_preview", { path, remap });
}

export function rbxmlImportCancel(): Promise<void> {
  return invoke("rbxml_import_cancel");
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

/** The saved rule of a smart playlist, to edit it. */
export function playlistRule(id: number): Promise<RuleNode> {
  return invoke("playlist_rule", { id });
}

export function playlistsMutate(op: PlaylistOp): Promise<PlaylistOutcome> {
  return invoke("playlists_mutate", { op });
}

export interface TagInfo {
  id: TagId;
  name: string;
  trackCount: number;
}

export interface TagCategory {
  id: TagId;
  name: string;
  tags: TagInfo[];
}

/** Categories and their tags, both in display order. */
export interface TagTree {
  categories: TagCategory[];
}

/**
 * A My Tags edit (`tag.rs`). `position` is an index among siblings. Names are
 * unique under one parent, case-insensitively; bad input is rejected whole.
 */
export type TagOp =
  | { type: "createCategory"; name: string; position?: number }
  | { type: "createTag"; categoryId: TagId; name: string; position?: number }
  | { type: "rename"; id: TagId; name: string }
  /** A category takes its tags with it; the tags are unassigned everywhere. */
  | { type: "delete"; id: TagId }
  /** A category: no `categoryId`; a tag: the target category, maybe a new one. */
  | { type: "move"; id: TagId; categoryId?: TagId | null; position: number }
  /** Idempotent; `changed` counts new links. */
  | { type: "assign"; trackIds: number[]; tagIds: TagId[] }
  | { type: "unassign"; trackIds: number[]; tagIds: TagId[] };

export interface TagOutcome {
  createdId: TagId | null;
  changed: number;
}

export function tagsGet(): Promise<TagTree> {
  return invoke("tags_get");
}

export function tagsMutate(op: TagOp): Promise<TagOutcome> {
  return invoke("tags_mutate", { op });
}

/** Output device names, the system default first (`audio_devices`). */
export function audioDevices(): Promise<string[]> {
  return invoke("audio_devices");
}

/** `beatgrid_*`, `waveform_columns`: the grid editor (reks-core `beatgrid.rs`, src-tauri `grid.rs`). */
export interface GridAnchor {
  positionMs: number;
  bpmX100: number;
  /** 1..beatsPerBar. */
  beatInBar: number;
  beatsPerBar: number;
}

export interface BeatMark {
  /** Microseconds from the start of the track. */
  positionUs: number;
  /** 1 = downbeat. */
  beatInBar: number;
}

export interface Beatgrid {
  /** Empty: no grid yet. More than one: a dynamic grid (only nudge and lock apply). */
  anchors: GridAnchor[];
  /** "rekordbox" | "analysis" | "manual"; null without a grid. */
  source: string | null;
  locked: boolean;
  /** Every beat line from the first anchor to the end of the track. */
  beats: BeatMark[];
}

/** One editor edit; the grid maths are in Rust. */
export type GridEdit =
  | { op: "nudge"; deltaMs: number }
  /** The beat nearest `atMs` becomes beat 1; beats do not move. */
  | { op: "setDownbeat"; atMs: number }
  | { op: "double" }
  | { op: "halve" }
  /** `anchorMs` places beat 1 only when the track has no grid yet. */
  | { op: "setBpm"; bpmX100: number; anchorMs: number }
  | { op: "setLocked"; locked: boolean };

export function beatgridGet(id: number): Promise<Beatgrid> {
  return invoke("beatgrid_get", { id });
}

export function beatgridEdit(id: number, edit: GridEdit): Promise<Beatgrid> {
  return invoke("beatgrid_edit", { id, edit });
}

/** Tempo × 100 of the newest run of taps (ms timestamps, oldest first); null below two taps. */
export function beatgridTapTempo(tapsMs: number[]): Promise<number | null> {
  return invoke("beatgrid_tap_tempo", { tapsMs: tapsMs.map(Math.round) });
}

/** Columns per second of `waveformColumns`. */
export const COLUMNS_PER_SECOND = 150;

/**
 * Waveform columns, 4 bytes each (`low, mid, high` 0..255, `mono` 0..31),
 * computed on first use (raw `ipc::Response`, not JSON).
 */
export function waveformColumns(id: number): Promise<ArrayBuffer> {
  return invoke("waveform_columns", { id });
}

/** `mix_*`: the Phase 5 mixer (src-tauri `mix.rs`, reks-mix `Control`). */
export const DECKS = 4;

export type CueRouting =
  | { type: "off" }
  /** Cue in the left ear, master (mono) in the right. */
  | { type: "splitMono" }
  /** 0-based output channels, e.g. master `[0, 1]`, cue `[2, 3]`. */
  | { type: "outputs"; master: [number, number]; cue: [number, number] };

export interface MixConfig {
  /** null: the system default. */
  device: string | null;
  /** null: 44.1 kHz when offered, else the device's default. */
  sampleRate: number | null;
  /** null: the device's default buffer. */
  bufferFrames: number | null;
  cue: CueRouting;
}

export interface MixDeviceOptions {
  rates: number[];
  defaultRate: number;
  /** Output channels the stream opens with. */
  channels: number;
  /** The cue routing to preselect: cue on 3/4 with four outputs, else off. */
  defaultCue: CueRouting;
  /** `[min, max]` frames, when the platform reports them. */
  bufferFrames: [number, number] | null;
}

/** One deck; times in ms of the track. */
export interface DeckView {
  trackId: number | null;
  loaded: boolean;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  cueMs: number;
  /** Tempo fader as a speed ratio. */
  tempo: number;
  nudge: number;
  speed: number;
  /** Tempo heard at the playhead; null without a grid. */
  bpm: number | null;
  /** Beat number at the playhead (0 at the first anchor); null without a grid. */
  beat: number | null;
  /** CUE is held from the cue point: playing until it is let go. */
  cuePreview: boolean;
  sync: boolean;
  syncBar: boolean;
  /** Following the master's tempo and phase. */
  locked: boolean;
  quantise: boolean;
  /** This deck is the sync master (`MixState.masterDeck`). */
  master: boolean;
  /** The active beat loop; null when not looping. */
  loopRegion: LoopView | null;
}

/** An active beat loop, in ms of the track. */
export interface LoopView {
  startMs: number;
  /** Where playback jumps back to `startMs`. */
  endMs: number;
  beats: number;
}

/** A peak meter: `level` linear (1 = 0 dBFS); `clip` reached 0 dBFS since the last state. */
export interface MeterView {
  level: number;
  clip: boolean;
}

/** Engine state, about 60 a second. */
export interface MixState {
  device: string;
  sampleRate: number;
  channels: number;
  bufferFrames: number | null;
  deviceLost: boolean;
  xruns: number;
  /**
   * The sync master deck; null when nobody is master. The first deck with a
   * grid to play takes it, or the one sent `takeMaster`. When it stops, ends
   * or is unloaded, a playing synced deck takes over at its current tempo
   * (its `tempo` changes to match); with none, nobody is master.
   */
  masterDeck: number | null;
  decks: DeckView[];
  /** Channel meters, after EQ and filter, before the fader. */
  meters: MeterView[];
  /** Master meter, before the limiter. */
  masterMeter: MeterView;
}

export interface HotCueView {
  /** The library cue, to rename or delete it. */
  id: number;
  /** 0..7 = A..H. */
  slot: number;
  positionMs: number;
  /** 0xRRGGBB from the library, null for the deck's default colour. */
  colorRgb: number | null;
  /** The cue's name from the library, shown on its pad. */
  name: string | null;
}

/** A memory cue or loop (by its start). */
export interface MemoryCueView {
  /** The library cue, to rename or delete it. */
  id: number;
  positionMs: number;
  /** A memory loop's end; null for a memory cue. */
  endMs: number | null;
  /** A memory loop playback enters by itself when the playhead reaches its start. */
  activeLoop: boolean;
  colorRgb: number | null;
  name: string | null;
}

/** A deck's cues after an edit. */
export interface DeckCues {
  hotCues: HotCueView[];
  /** By position. */
  memoryCues: MemoryCueView[];
}

/**
 * Mirrors `src-tauri` `mix::CueEdit`: an edit to the loaded track's cues,
 * stored in the library (so exports carry it) and applied to the deck.
 * Cues set at the playhead snap to the beat with quantise.
 */
export type CueEdit =
  /** An empty hot cue slot (0..7 = A..H) at the playhead, in the next palette colour. */
  | { type: "setHotCue"; slot: number }
  /** A memory cue at the playhead; while a loop plays, a memory loop of it. */
  | { type: "addMemoryCue" }
  /** A memory loop playback enters by itself (rekordbox's active loop), or not. */
  | { type: "setActiveLoop"; cueId: number; on: boolean }
  /** null or blank clears the name. */
  | { type: "rename"; cueId: number; name: string | null }
  | { type: "delete"; cueId: number };

export interface DeckLoaded {
  deck: number;
  trackId: number;
  durationMs: number;
  hotCues: HotCueView[];
  /** By position. */
  memoryCues: MemoryCueView[];
}

export type ChannelParam =
  | "trim"
  | "eqHi"
  | "eqMid1"
  | "eqMid2"
  | "eqLo"
  | "fader"
  /** 0 = X, 0.5 = off, 1 = Y. */
  | "filterAssign"
  /** 0 = X, 0.5 = off (direct), 1 = Y. */
  | "xfadeAssign"
  | "cue";

export type FilterUnit = "x" | "y";

export type FilterParam =
  | "freq"
  | "resonance"
  | "highPass"
  | "bandPass"
  | "lowPass"
  | "on"
  /** Unit Y only: Y follows X's FREQ. */
  | "link";

export type MasterParam = "level" | "cueLevel" | "cueMix" | "splitCue" | "cuePostEq";

/**
 * Mirrors `reks_mix::Control` (knob values 0..1, switches 0 or 1). `Load` is
 * `mixLoad`; `seek` takes ms. Mixer controls take effect once the channel
 * strips are wired (phase 5 tasks 6–7).
 */
export type MixControl =
  | { type: "unload"; deck: number }
  | { type: "play"; deck: number }
  | { type: "pause"; deck: number }
  /** CUE pressed (CDJ): playing → back to the cue point; paused on it → preview; else set it. */
  | { type: "cue"; deck: number }
  /** CUE let go: ends a cue preview. */
  | { type: "cueRelease"; deck: number }
  | { type: "seek"; deck: number; ms: number }
  | { type: "hotCue"; deck: number; slot: number }
  /** Memory cue call: the cue point and playhead move to `ms` exactly; a playing deck plays on. */
  | { type: "callCue"; deck: number; ms: number }
  /** Speed ratio, 1 = as recorded. */
  | { type: "tempo"; deck: number; ratio: number }
  /** Pitch bend while held; 1 releases it. */
  | { type: "nudge"; deck: number; ratio: number }
  | { type: "sync"; deck: number; on: boolean; bar: boolean }
  /** MASTER: this deck becomes the sync master until it stops, ends or is unloaded. */
  | { type: "takeMaster"; deck: number }
  /** Signed beats along the grid (the pads' `JUMP_PAGES`, FINE = 1/32); during a loop the loop moves too. */
  | { type: "beatJump"; deck: number; beats: number }
  /** A fixed 10 ms (the deck's FINE pad is a 1/32 `beatJump` instead). */
  | { type: "fineJump"; deck: number; forward: boolean }
  /** Loop of `beats` (the pads' `LOOP_PAGES`) from the playhead, from the beat with quantise. */
  | { type: "beatLoop"; deck: number; beats: number }
  | { type: "loopExit"; deck: number }
  /** Halve (`double: false`) or double the active loop. */
  | { type: "loopResize"; deck: number; double: boolean }
  | { type: "quantise"; deck: number; on: boolean }
  | { type: "channel"; ch: number; param: ChannelParam; value: number }
  | { type: "filter"; unit: FilterUnit; param: FilterParam; value: number }
  | { type: "crossfader"; pos: number; curve: number }
  | { type: "master"; param: MasterParam; value: number }
  /** The player's volume slider, 0..1 (squared taper in Rust), on top of the master level. */
  | { type: "volume"; value: number };

export function mixDeviceOptions(device: string | null): Promise<MixDeviceOptions> {
  return invoke("mix_device_options", { device });
}

/** Opens the output and streams the state to `onState` until `mixStop`; resolves to the first state. */
export function mixStart(config: MixConfig, onState: (s: MixState) => void): Promise<MixState> {
  const channel = new Channel<MixState>();
  channel.onmessage = onState;
  return invoke("mix_start", { config, onState: channel });
}

export function mixStop(): Promise<void> {
  return invoke("mix_stop");
}

/** Decodes and loads track `id` on `deck`; null when a newer load for the deck won. */
export function mixLoad(deck: number, id: number): Promise<DeckLoaded | null> {
  return invoke("mix_load", { deck, id });
}

export function mixControl(control: MixControl): Promise<void> {
  return invoke("mix_control", { control });
}

/** Edits the cues of the track on `deck`; resolves to the deck's cues after it. */
export function mixCueEdit(deck: number, edit: CueEdit): Promise<DeckCues> {
  return invoke("mix_cue_edit", { deck, edit });
}

/** `settings_*`, `library_root*`, `cache_dir_move` (reks-core `settings.rs`). */
export type KeyNotation = "classical" | "camelot" | "openKey";

/** How the decks draw their waveforms; all come from the same analysis columns. */
export type DeckWaveform = "threeBand" | "rgb" | "blue";

/** What SYNC lines up with the master: the beat, or the bar (downbeats too). */
export type SyncAlign = "beat" | "bar";

/** Library window state kept across launches. */
export interface UiState {
  /** Hidden column ids; `null` until changed (UI defaults). */
  hiddenColumns: string[] | null;
  filterBarOpen: boolean;
  filtersEnabled: boolean;
  infoOpen: boolean;
  /** 0..1. */
  volume: number;
}

export interface Settings {
  /** `null`: the system default. */
  audioDevice: string | null;
  /** `null`: 44.1 kHz when offered, else the device's default. */
  audioSampleRate: number | null;
  /** `null`: the device's default buffer. */
  audioBufferFrames: number | null;
  /** `null` until the user picks one: each device gets its own default. */
  audioCue: CueRouting | null;
  keyNotation: KeyNotation;
  deckWaveform: DeckWaveform;
  syncAlign: SyncAlign;
  /** The cache dir in use. */
  cacheDir: string;
  ui: UiState;
  /** Whose analysis a USB export writes when rekordbox and reks both have a track. */
  anlzPreference: AnlzPreference;
  /** This build has the OneLibrary key built in; the key itself never leaves Rust. */
  onelibraryKeyBuiltin: boolean;
}

/** Absent fields stay; `null` goes back to the default (system device, automatic rate, device buffer). */
export interface SettingsPatch {
  audioDevice?: string | null;
  audioSampleRate?: number | null;
  audioBufferFrames?: number | null;
  audioCue?: CueRouting;
  keyNotation?: KeyNotation;
  deckWaveform?: DeckWaveform;
  syncAlign?: SyncAlign;
  ui?: UiState;
  anlzPreference?: AnlzPreference;
}

export interface LibraryRoot {
  id: number;
  path: string;
  /** UTC ISO-8601. */
  lastScan: string | null;
}

export function settingsGet(): Promise<Settings> {
  return invoke("settings_get");
}

export function settingsSet(patch: SettingsPatch): Promise<Settings> {
  return invoke("settings_set", { patch });
}

export function libraryRoots(): Promise<LibraryRoot[]> {
  return invoke("library_roots");
}

export function libraryRootAdd(path: string): Promise<LibraryRoot> {
  return invoke("library_root_add", { path });
}

/** Stops watching the folder; its tracks stay. */
export function libraryRootRemove(id: number): Promise<void> {
  return invoke("library_root_remove", { id });
}

/** Moves the cache (artwork, waveforms) to `path` and returns the new settings. */
export function cacheDirMove(path: string): Promise<Settings> {
  return invoke("cache_dir_move", { path });
}

/** File system of a volume (`device::FsKind`). The planner turns it into warnings. */
export type FsKind =
  | { kind: "fat32" }
  | { kind: "exFat" }
  | { kind: "hfs" }
  /** `name`: the OS's name for it (`apfs`, `ntfs`, …). */
  | { kind: "other"; name: string };

/** A mounted volume (`device::DeviceInfo`). */
export interface DeviceInfo {
  mountPoint: string;
  fs: FsKind;
  /** The OS's file system string (`msdos`, `vfat`, `FAT32`). */
  fsName: string;
  removable: boolean;
  totalBytes: number;
  freeBytes: number;
  /** May be empty. */
  label: string;
  osName: string;
}

/** Mounted volumes the OS reports as removable. */
export function devicesList(): Promise<DeviceInfo[]> {
  return invoke("devices_list");
}

/** Unmounts and ejects the removable volume at `mountPoint`. */
export function deviceEject(mountPoint: string): Promise<void> {
  return invoke("device_eject", { mountPoint });
}

/** Player family the export is checked against (`TargetProfile`). */
export type TargetProfile = "Cdj2000Nxs" | "Cdj2000Nxs2" | "Cdj3000" | "XdjXz" | "Generic";

/** A known stick with a saved profile (`devices::Device`, D27). */
export interface Device {
  id: number;
  /** The OS's volume ID (D28); null when the OS gives none. */
  volumeSerial: string | null;
  label: string;
  sizeBytes: number;
  /** User-given. */
  name: string;
  target: TargetProfile;
  createdAt: string;
  lastSyncedAt: string | null;
}

/** What a mounted stick is to this library (`devices::DeviceMatch`); every string is final. */
export type DeviceMatch =
  | { type: "known"; device: Device }
  | {
      type: "ambiguous";
      volumeSerial: string | null;
      reason: "serialChanged" | "noSerial";
      candidates: Device[];
      prompt: string;
      note: string;
    }
  | {
      /** `foreign`: holds a rekordbox library reks did not write. */
      type: "new" | "foreign";
      volumeSerial: string | null;
      suggestedName: string;
      prompt: string;
      note: string;
    };

/** How a stick playlist compares with the profile (`devices::SyncState`). */
export type SyncState =
  | { state: "inSync" }
  | {
      state: "changed";
      added: number;
      removed: number;
      reordered: boolean;
      movedFrom: string[] | null;
    }
  | { state: "new" }
  | { state: "willRemove" };

export interface PlaylistStatus {
  playlistId: number | null;
  path: string[];
  pathText: string;
  tracks: number;
  state: SyncState;
  text: string;
}

/** A ticked profile entry and what unticking it frees. */
export interface EntryStatus {
  playlistId: number;
  name: string;
  pathText: string;
  isFolder: boolean;
  playlists: number;
  tracks: number;
  freesBytes: number;
  freesText: string;
}

/** Status of a stick profile (`devices::ProfileStatus`); every string is final. */
export interface ProfileStatus {
  device: Device;
  source: "stick" | "lastSync" | "neverSynced";
  sourceText: string;
  playlists: PlaylistStatus[];
  entries: EntryStatus[];
  plannedBytes: number;
  capacityBytes: number;
  overCapacity: boolean;
  capacityText: string;
  summary: string;
  notes: string[];
}

/** Known sticks, in the order they were added. */
export function profilesList(): Promise<Device[]> {
  return invoke("profiles_list");
}

/** Identifies the mounted stick `info` (asks the OS for its volume ID). */
export function profileIdentify(info: DeviceInfo): Promise<DeviceMatch> {
  return invoke("profile_identify", { info });
}

/** Remembers the mounted stick `info` as a new stick with an empty profile. */
export function profileCreate(
  info: DeviceInfo,
  volumeSerial: string | null,
  name: string,
  target: TargetProfile,
): Promise<Device> {
  return invoke("profile_create", { info, volumeSerial, name, target });
}

/** Confirms an `ambiguous` match: the stick `info` is `id` (its stored volume ID is replaced). */
export function profileLink(
  id: number,
  info: DeviceInfo,
  volumeSerial: string | null,
): Promise<Device> {
  return invoke("profile_link", { id, info, volumeSerial });
}

export function profileRename(id: number, name: string): Promise<Device> {
  return invoke("profile_rename", { id, name });
}

export function profileSetTarget(id: number, target: TargetProfile): Promise<Device> {
  return invoke("profile_set_target", { id, target });
}

/** The stick's profile as an export selection, in library tree order. */
export function profileSelection(id: number): Promise<ExportSelection> {
  return invoke("profile_selection", { id });
}

/** Saves the stick's profile: these playlists and folders. */
export function profileSetSelection(id: number, playlists: number[]): Promise<void> {
  return invoke("profile_set_selection", { id, playlists });
}

/** Profile status; reads the stick at `mountPoint` when it is plugged in. */
export function profileStatus(id: number, mountPoint: string | null): Promise<ProfileStatus> {
  return invoke("profile_status", { id, mountPoint });
}

/**
 * Where an export takes a track's ANLZ when rekordbox's cache and reks's own analysis both have
 * it (`reks_core::AnlzPreference`, phase 3 task 10). reks-built ANLZ is UNVERIFIED on players.
 */
export type AnlzPreference = "rekordboxFirst" | "reksFirst" | "rekordboxOnly";

/** Tracks with no analysis at all (`MissingAnalysis`). The writer refuses `exportWithout`. */
export type MissingAnalysis = "skip" | "generate" | "exportWithout";

/**
 * What an export does with the stick's play history (`HistoryPolicy`, D23). `import` (the
 * export guard's "Import now") imports the sessions the library lacks, then backs up.
 */
export type HistoryPolicy = "import" | "backUp" | "discard";

/** `plan::ExportOptions`. */
export interface ExportOptions {
  target: TargetProfile;
  keepStale: boolean;
  missingAnalysis: MissingAnalysis;
  artwork: boolean;
  skipUnsupported: boolean;
  history: HistoryPolicy;
  /** Required for `discard` on a stick that holds history. */
  discardHistoryConfirmed: boolean;
  /** `history` was picked in the export guard; without it unimported sessions block the plan. */
  historyChosen: boolean;
  /** The `export.dividers` flag: playlist dividers as silent tracks (UNVERIFIED, D22). */
  dividers: boolean;
  /** Seeded from and saved to the `anlzPreference` setting by the wizard. */
  anlzPreference: AnlzPreference;
  /** Also write `exportLibrary.db` (OneLibrary, experimental, phase 4). Off by default. */
  onelibrary: boolean;
  /** The `export.active_loops` flag: each track's earliest active memory loop as its active loop (UNVERIFIED). */
  activeLoops: boolean;
}

/** Playlists and folders (= everything under them) to export (`plan::Selection`). */
export interface ExportSelection {
  playlists: number[];
}

/** `dryrun::Fact`: a label and its value, worded in Rust. */
export interface Fact {
  label: string;
  value: string;
}

export interface DryRunFileRow {
  path: string;
  from: string | null;
  sizeText: string;
}

export interface DryRunFileGroup {
  count: number;
  bytes: number;
  totalText: string;
  files: DryRunFileRow[];
}

/** Notices of one kind (`dryrun::NoticeGroup`); `items` is empty for one-sentence kinds. */
export interface NoticeGroup {
  kind: string;
  title: string;
  items: string[];
}

/** Stick sessions the library has not imported, and the choice the export needs. */
export interface HistoryGuardView {
  title: string;
  text: string;
  /** `HISTORY 001: 12 plays`. */
  sessions: string[];
  /** The user has chosen; `history.policy` is the choice. */
  chosen: boolean;
  /** "Import now" can run. */
  canImport: boolean;
}

/** The dry-run view of an export plan (`dryrun::DryRun`); every string is final. */
export interface DryRun {
  /** The kept plan this shows; `exportApply` writes exactly it. */
  planId: number;
  ready: boolean;
  status: string;
  device: {
    mountPoint: string;
    name: string;
    fsText: string;
    spaceText: string;
    volumeSerial: string | null;
  };
  target: string;
  summary: Fact[];
  playlists: {
    id: number;
    name: string;
    depth: number;
    isFolder: boolean;
    /** Written as a fixed list of the tracks it matches now (D29). */
    smart: boolean;
    detail: string;
  }[];
  copies: DryRunFileGroup;
  deletes: DryRunFileGroup;
  space: {
    fits: boolean;
    totalBytes: number;
    freeBytes: number;
    writeBytes: number;
    deleteBytes: number;
    freeAfterBytes: number;
    usedAfterBytes: number;
    facts: Fact[];
  };
  history: {
    atRisk: boolean;
    policy: HistoryPolicy;
    title: string;
    text: string;
    actionText: string | null;
    backups: DryRunFileGroup;
    /** The export guard (`dryrun::GuardView`): sessions not imported yet; null when none. */
    guard: HistoryGuardView | null;
  };
  problems: NoticeGroup[];
  warnings: NoticeGroup[];
  /** Tracks whose audio issue the converter's Older players preset fixes (phase 2.6). */
  convertible: number[];
  /** One sentence on them; null when there are none. */
  convertText: string | null;
  /** The experimental OneLibrary step (`dryrun::OneLibraryView`); null when off or unplanned. */
  onelibrary: OneLibraryView | null;
}

/** What the OneLibrary step writes (`exportLibrary.db`) and removes beside it. */
export interface OneLibraryView {
  title: string;
  text: string;
  write: DryRunFileRow;
  removes: DryRunFileGroup;
}

/**
 * Plans an export to the stick at `mountPoint` without writing anything (slow: reads the stick
 * and the files). `deviceId`: the stick profile the export records its sync in.
 */
export function exportPlan(
  mountPoint: string,
  selection: ExportSelection,
  options: ExportOptions,
  deviceId: number | null = null,
): Promise<DryRun> {
  return invoke("export_plan", { mountPoint, selection, options, deviceId });
}

/** Steps of an export (`apply::ApplyStage`), in order. */
export type ApplyStage =
  | "preflight"
  | "importHistory"
  | "backup"
  | "write"
  | "oneLibrary"
  | "sync"
  | "validate"
  | "manifest";

/** Where a running export is (`report::ProgressView`); every string is final. */
export interface ExportProgress {
  stage: ApplyStage;
  title: string;
  /** `Track 3 of 40: Alpha – One`. */
  detail: string | null;
  /** Tracks written of `total`; `total` 0 = no count. */
  done: number;
  total: number;
  /** False once the new database is being written: cancelling no longer keeps the old one. */
  cancellable: boolean;
  warnings: string[];
}

export type ExportOutcome = "success" | "problems" | "cancelled" | "failed";

/** The export's report (`report::ReportView`); every string is final. */
export interface ExportReportView {
  outcome: ExportOutcome;
  title: string;
  status: string;
  mountPoint: string;
  deviceName: string;
  facts: Fact[];
  history: {
    title: string;
    text: string;
    /** The backup folder in the app data dir. */
    folder: string | null;
    files: string[];
  };
  problems: NoticeGroup[];
  warnings: NoticeGroup[];
}

/**
 * Writes the plan dry run `planId` showed to its stick, streaming progress.
 * Rejects only when that plan is gone (replanned or already exported); a
 * refused or failed export resolves with outcome `failed`.
 */
export function exportApply(
  planId: number,
  onEvent: (p: ExportProgress) => void,
): Promise<ExportReportView> {
  const channel = new Channel<ExportProgress>();
  channel.onmessage = onEvent;
  return invoke("export_apply", { planId, onEvent: channel });
}

/** Asks the running export to stop (it keeps the old database while `cancellable`). */
export function exportCancel(): Promise<void> {
  return invoke("export_cancel");
}

/** Converter output container (`reks_audio::Target`). */
export type ConvertTarget = "wav" | "aiff" | "flac";
/** `reks_audio::Depth`: `keep` = the source's (16 for 16 bits or fewer, else 24). */
export type ConvertDepth = "keep" | "bits16" | "bits24";
/** `reks_audio::Rate`. */
export type ConvertRate = "keep" | "hz44100" | "hz48000";

/** `reks_audio::ConvertSpec`. */
export interface ConvertSpec {
  target: ConvertTarget;
  depth: ConvertDepth;
  rate: ConvertRate;
}

/** `convert::ConvertOptions`. */
export interface ConvertOptions {
  destination: "nextToOriginal" | { folder: string };
  library: "replace" | "addAsNew";
  /** `trash` needs `replace`. */
  originals: "keep" | "trash";
  /** With rate `keep`, sources above 48 kHz go to 44.1 or 48 kHz (Older players). */
  capRate48k: boolean;
}

/** `convert::view::Fact`. */
export interface ConvertFact {
  label: string;
  value: string;
}

/** The converter's dry run (`convert::view::PlanView`); every string is final. */
export interface ConvertDryRun {
  /** The kept plan this shows; `convertApply` runs exactly it. */
  planId: number;
  ready: boolean;
  status: string;
  facts: ConvertFact[];
  items: {
    trackId: number;
    label: string;
    source: string;
    dest: string;
    from: string;
    to: string;
    sizeText: string;
  }[];
  skipped: ConvertSkip[];
  warnings: string[];
}

export interface ConvertSkip {
  trackId: number;
  label: string;
  reason: string;
}

/** Progress of a running conversion (`convert::ConvertEvent`). */
export type ConvertEvent =
  | { type: "started"; trackId: number; index: number; total: number }
  | { type: "progress"; trackId: number; fraction: number }
  | { type: "converted"; trackId: number; newTrackId: number; dest: string }
  | { type: "failed"; trackId: number; message: string };

/** The converter's report (`convert::view::ReportView`); every string is final. */
export interface ConvertReportView {
  outcome: "success" | "problems" | "cancelled";
  title: string;
  status: string;
  facts: ConvertFact[];
  converted: {
    trackId: number;
    newTrackId: number;
    label: string;
    dest: string;
    original: string;
  }[];
  failed: { trackId: number; label: string; message: string }[];
  skipped: ConvertSkip[];
}

/** Plans converting `tracks`; reads file headers only, writes nothing. */
export function convertPlan(
  tracks: number[],
  spec: ConvertSpec,
  options: ConvertOptions,
): Promise<ConvertDryRun> {
  return invoke("convert_plan", { tracks, spec, options });
}

/**
 * Runs the plan dry run `planId` showed, streaming progress. Rejects only when that plan is
 * gone (replanned or already run); per-file failures are in the report.
 */
export function convertApply(
  planId: number,
  onEvent: (e: ConvertEvent) => void,
): Promise<ConvertReportView> {
  const channel = new Channel<ConvertEvent>();
  channel.onmessage = onEvent;
  return invoke("convert_apply", { planId, onEvent: channel });
}

/** Asks the running conversion to stop after cleaning up its current file. */
export function convertCancel(): Promise<void> {
  return invoke("convert_cancel");
}

/** `history_*` commands: imported play history (phase-2.5.md, D23). */
export interface SessionQuery {
  /** true: practice only; false: counted only; absent: all. */
  excluded?: boolean;
}

/** One imported session; newest first, undated last. */
/** The stick library a session was logged in: `export.pdb` or OneLibrary's `exportLibrary.db`. */
export type HistoryLibrary = "deviceLibrary" | "oneLibrary";

export interface SessionRow {
  id: number;
  name: string;
  /** As on the stick, e.g. `HISTORY 001`. */
  sourceName: string;
  library: HistoryLibrary;
  /** `YYYY-MM-DD`; null until history dates are decoded (HIST1). */
  playedOn: string | null;
  venue: string | null;
  /** Practice: left out of play counts. */
  excluded: boolean;
  excludedReason: "user" | "pre_baseline" | null;
  /** Track entries (dividers not counted). */
  tracks: number;
  /** Track entries with no library track. */
  unmatched: number;
  importedAt: string;
}

export type EntryKind = "track" | "divider";

export interface SessionEntryRow {
  position: number;
  kind: EntryKind;
  /** null: unmatched (or a divider). */
  trackId: number | null;
  title: string | null;
  artist: string | null;
  durationMs: number | null;
  bpmX100: number | null;
  key: Key | null;
  /** As the stick has them. */
  rawTitle: string | null;
  rawArtist: string | null;
  rawPath: string | null;
  rawDurationMs: number | null;
}

export interface SessionDetail {
  session: SessionRow;
  notes: string | null;
  volumeSerial: string | null;
  /** Play order. */
  entries: SessionEntryRow[];
}

/** A session a track was played in. */
export interface SessionRef {
  id: number;
  name: string;
  playedOn: string | null;
  venue: string | null;
  excluded: boolean;
  plays: number;
}

/** A session edit: absent fields are unchanged; `null` venue/notes clear them. */
export interface SessionPatch {
  name?: string;
  venue?: string | null;
  notes?: string | null;
  /** Practice. */
  excluded?: boolean;
}

export type MatchKind = "manifest" | "pathSuffix" | "metadata";

export interface PlannedEntry {
  position: number;
  kind: EntryKind;
  trackId: number | null;
  matchedBy: MatchKind | null;
  raw: {
    stickPath: string | null;
    title: string | null;
    artist: string | null;
    durationMs: number | null;
  };
}

export type SessionAction =
  | { kind: "new" }
  | { kind: "skip"; reason: "alreadyImported"; sessionId: number }
  | { kind: "skip"; reason: "duplicateInSource" }
  | { kind: "extend"; sessionId: number; existingEntries: number };

export interface PlannedSession {
  library: HistoryLibrary;
  sourceName: string;
  playedOn: string | null;
  /** Key of `ImportChoices.excluded`. */
  fingerprint: string;
  action: SessionAction;
  entries: PlannedEntry[];
}

export interface HistoryImportPlan {
  volumeSerial: string | null;
  baselineDate: string | null;
  sessions: PlannedSession[];
}

export type HistorySource = { kind: "stick"; path: string } | { kind: "backup"; path: string };

/** A mounted stick, or a folder (a Phase 2 backup or a stick copy). */
export type ScanSource = { kind: "stick"; device: DeviceInfo } | { kind: "folder"; path: string };

/** What became of the source's `exportLibrary.db` (read with the OneLibrary key). */
export type OneLibraryStatus =
  | { status: "absent" }
  | { status: "read"; sessions: number }
  | { status: "notRead"; reason: string };

export interface HistoryScan {
  planId: number;
  source: HistorySource;
  onelibrary: OneLibraryStatus;
  plan: HistoryImportPlan;
}

export interface ImportChoices {
  /** Fingerprint → Practice. Absent: the plan's default. */
  excluded: Record<string, boolean>;
}

export interface HistoryImportReport {
  created: number[];
  extended: number[];
  skipped: number;
  entries: number;
  unmatched: number;
}

export function historySessions(query: SessionQuery = {}): Promise<SessionRow[]> {
  return invoke("history_sessions", { query });
}

export function historySession(id: number): Promise<SessionDetail> {
  return invoke("history_session", { id });
}

export function historySessionUpdate(id: number, patch: SessionPatch): Promise<void> {
  return invoke("history_session_update", { id, patch });
}

export function historySessionDelete(id: number): Promise<void> {
  return invoke("history_session_delete", { id });
}

/** "Locate…": links entry `position` of `session` to library track `track`. */
export function historyLinkEntry(session: number, position: number, track: number): Promise<void> {
  return invoke("history_link_entry", { session, position, track });
}

export function historyTrackSessions(id: number): Promise<SessionRef[]> {
  return invoke("history_track_sessions", { id });
}

/** Read-only dry run of importing a stick's or backup's history. */
export function historyScan(source: ScanSource): Promise<HistoryScan> {
  return invoke("history_scan", { source });
}

/** Imports the dry run `planId` showed, with the Practice choices. */
export function historyImport(
  planId: number,
  choices: ImportChoices,
): Promise<HistoryImportReport> {
  return invoke("history_import", { planId, choices });
}

// Adopt a foreign stick (phase 2.5 task 12, D27; `reks_export_rb::adopt`).

/** One stick playlist entry with its library match. */
export interface AdoptEntry {
  /** Null when the entry's track row is missing on the stick. */
  stickPath: string | null;
  title: string | null;
  artist: string | null;
  kind: EntryKind;
  trackId: number | null;
  matchedBy: MatchKind | null;
}

/** A library playlist Link may name: same name, same tracks in the same order. */
export interface LinkCandidate {
  playlistId: number;
  pathText: string;
}

/** What to do with one stick playlist. */
export type AdoptChoice =
  | { kind: "link"; playlistId: number }
  | { kind: "create" }
  | { kind: "skip" };

/** One stick playlist in the adopt dry run; every string is final. */
export interface AdoptPlaylist {
  /** Row id in `export.pdb`; the key of `AdoptChoices.playlists`. */
  pdbId: number;
  path: string[];
  pathText: string;
  entries: AdoptEntry[];
  matched: number;
  unmatched: number;
  dividers: number;
  /** Empty unless an exact ordered match exists. */
  link: LinkCandidate[];
  defaultChoice: AdoptChoice;
  text: string;
}

export interface UnmatchedTrack {
  stickPath: string | null;
  title: string | null;
  artist: string | null;
}

/** The adopt dry run (`adopt::AdoptPlan`); nothing is written. */
export interface AdoptPlan {
  volumeSerial: string | null;
  writtenByReks: boolean;
  /** Imported first by `adoptApply`; null when it could not be read. */
  history: HistoryImportPlan | null;
  historySessions: number;
  historyText: string;
  playlists: AdoptPlaylist[];
  unmatched: UnmatchedTrack[];
  summary: string;
  unmatchedText: string;
  notes: string[];
}

export interface AdoptScan {
  planId: number;
  plan: AdoptPlan;
}

export interface AdoptChoices {
  /** The stick's profile (created or linked first). */
  deviceId: number;
  /** `pdbId` → choice. Absent: the plan's default (Create). */
  playlists: Record<number, AdoptChoice>;
  /** Practice marks for the history import. */
  history: ImportChoices;
}

export interface AdoptReport {
  history: HistoryImportReport | null;
  folderId: number | null;
  created: number[];
  linked: number[];
  skipped: number;
  unmatched: UnmatchedTrack[];
  summary: string;
}

/** Read-only dry run of adopting the mounted stick `info`. */
export function adoptScan(info: DeviceInfo): Promise<AdoptScan> {
  return invoke("adopt_scan", { info });
}

/** Applies the dry run `planId` showed: history first, then playlists and the stick's profile. */
export function adoptApply(planId: number, choices: AdoptChoices): Promise<AdoptReport> {
  return invoke("adopt_apply", { planId, choices });
}
