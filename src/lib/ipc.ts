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
  | { type: "reorderEntries"; playlistId: number; positions: number[]; before: number }
  /** Before the entry at `position`; absent or null = appended. Label rules in reks-core. */
  | { type: "insertDivider"; playlistId: number; position?: number | null; label: string }
  | { type: "renameDivider"; playlistId: number; position: number; label: string };

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

/** `settings_*`, `library_root*`, `cache_dir_move` (reks-core `settings.rs`). */
export type KeyNotation = "classical" | "camelot" | "openKey";

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
  keyNotation: KeyNotation;
  /** The cache dir in use. */
  cacheDir: string;
  ui: UiState;
}

/** Absent fields stay; `audioDevice: null` goes back to the system default. */
export interface SettingsPatch {
  audioDevice?: string | null;
  keyNotation?: KeyNotation;
  ui?: UiState;
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

/** Tracks without rekordbox analysis (`MissingAnalysis`). The writer refuses `exportWithout`. */
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
  playlists: { id: number; name: string; depth: number; isFolder: boolean; detail: string }[];
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
