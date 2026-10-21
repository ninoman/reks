/**
 * Dev-only stand-in for the Rust commands, so the UI runs in a plain browser
 * (`npm run dev`, then open http://localhost:1420): generated tracks,
 * playlists, settings and an import preview. Loaded by `main.tsx` only in
 * dev builds outside Tauri; never part of a release bundle.
 */

import type { Channel } from "@tauri-apps/api/core";
import { mockIPC } from "@tauri-apps/api/mocks";
import type {
  ConvertDryRun,
  ConvertEvent,
  ConvertOptions,
  ConvertReportView,
  ConvertSpec,
  Device,
  DeviceInfo,
  DeviceMatch,
  DryRun,
  ExportOptions,
  ExportProgress,
  ExportReportView,
  ExportSelection,
  Facets,
  ImportPreview,
  ImportReport,
  KeyInfo,
  PlayerStatus,
  PlaylistNode,
  PlaylistOp,
  PlaylistStatus,
  ProfileStatus,
  Settings,
  SettingsPatch,
  SortColumn,
  TagOp,
  TrackDetail,
  TrackQuery,
  TrackRow,
} from "../lib/ipc";
import { mockAdopt } from "./mockAdopt";
import { mockGrid } from "./mockGrid";
import { mockHistory } from "./mockHistory";
import { mockMix } from "./mockMix";
import { createMockSmart } from "./mockSmart";
import { createMockTags } from "./mockTags";

const CAMELOT: [string, string][] = [
  ["Am", "8A"],
  ["C", "8B"],
  ["Em", "9A"],
  ["G", "9B"],
  ["Bm", "10A"],
  ["D", "10B"],
  ["F#m", "11A"],
  ["A", "11B"],
  ["Dbm", "12A"],
  ["E", "12B"],
  ["Abm", "1A"],
  ["B", "1B"],
  ["Ebm", "2A"],
  ["F#", "2B"],
  ["Bbm", "3A"],
  ["Db", "3B"],
  ["Fm", "4A"],
  ["Ab", "4B"],
  ["Cm", "5A"],
  ["Eb", "5B"],
  ["Gm", "6A"],
  ["Bb", "6B"],
  ["Dm", "7A"],
  ["F", "7B"],
];
const KEYS: KeyInfo[] = CAMELOT.map(([key, camelot]) => {
  const n = Number.parseInt(camelot, 10);
  const open = ((n - 8 + 12) % 12) + 1;
  return { key, camelot, openKey: `${open}${camelot.endsWith("A") ? "m" : "d"}` };
});
const GENRES = ["House", "Deep House", "Techno", "Disco", "Electro", "Breaks"];
const COLOURS = ["pink", "red", "orange", "yellow", "green", "aqua", "blue", "purple"];
const ARTISTS = [
  "Moodymann",
  "Kerri Chandler",
  "DJ Koze",
  "Roza Terenzi",
  "Ron Trent",
  "Avalon Emerson",
  "Floorplan",
  "Peggy Gou",
];
const WORDS = [
  "Night",
  "Sun",
  "Glass",
  "Motion",
  "River",
  "Echo",
  "Velvet",
  "Signal",
  "Dawn",
  "Orbit",
  "Ember",
  "Tide",
];

/** Small deterministic PRNG so every reload shows the same library. */
function rng(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1664525 + 1013904223) % 4294967296;
    return s / 4294967296;
  };
}
const rand = rng(7);
const pick = <T>(list: T[]): T => list[Math.floor(rand() * list.length)] as T;

const TRACKS: TrackRow[] = Array.from({ length: 3000 }, (_, i) => ({
  id: i + 1,
  title: `${pick(WORDS)} ${pick(WORDS)}${rand() < 0.3 ? " (Extended Mix)" : ""}`,
  artist: pick(ARTISTS),
  album: rand() < 0.6 ? `${pick(WORDS)} EP` : null,
  genre: pick(GENRES),
  label: rand() < 0.5 ? `${pick(WORDS)} Records` : null,
  remixer: null,
  comment: rand() < 0.2 ? "Peak time" : null,
  key: rand() < 0.9 ? pick(KEYS).key : null,
  bpmX100: Math.round((118 + rand() * 14) * 100),
  rating: Math.floor(rand() * 6),
  color: rand() < 0.35 ? pick(COLOURS) : null,
  durationMs: Math.round((240 + rand() * 300) * 1000),
  year: 1995 + Math.floor(rand() * 30),
  kind: pick(["mp3", "flac", "aiff", "m4a"]),
  bitRateKbps: 320,
  dateAdded: `2026-0${1 + Math.floor(rand() * 9)}-1${Math.floor(rand() * 9)}T10:00:00Z`,
  playCount: 0,
  lastPlayed: null,
  missing: rand() < 0.02,
  needsAnalysis: rand() < 0.08,
  entryPosition: null,
  tags: [],
}));

// Baseline play counts (as from rekordbox XML), before the mock sessions add theirs.
for (const t of TRACKS) {
  if (rand() < 0.35) continue;
  t.playCount = 1 + Math.floor(rand() * 30);
  t.lastPlayed = `2025-${String(1 + Math.floor(rand() * 12)).padStart(2, "0")}-1${Math.floor(rand() * 9)}`;
}
const history = mockHistory(TRACKS);
const tags = createMockTags(TRACKS.length);

/** A row as the backend returns it: with its current tags. */
const tagged = (r: TrackRow): TrackRow => ({ ...r, tags: tags.tagsOf(r.id) });

const NODES: PlaylistNode[] = [
  {
    id: 1,
    parentId: null,
    name: "Gigs",
    isFolder: true,
    depth: 0,
    position: 0,
    entries: 0,
    smart: false,
  },
  {
    id: 2,
    parentId: 1,
    name: "Warm-up",
    isFolder: false,
    depth: 1,
    position: 0,
    entries: 40,
    smart: false,
  },
  {
    id: 3,
    parentId: 1,
    name: "Peak time",
    isFolder: false,
    depth: 1,
    position: 1,
    entries: 55,
    smart: false,
  },
  {
    id: 4,
    parentId: null,
    name: "Digging 2026",
    isFolder: false,
    depth: 0,
    position: 1,
    entries: 120,
    smart: false,
  },
  {
    id: 5,
    parentId: null,
    name: "Closing",
    isFolder: false,
    depth: 0,
    position: 2,
    entries: 25,
    smart: false,
  },
];
/** A playlist entry: a track id or a divider. */
type MockEntry = number | { label: string };
const PLAYLISTS: Record<number, MockEntry[]> = Object.fromEntries(
  NODES.filter((n) => !n.isFolder).map((n) => [
    n.id,
    Array.from({ length: n.entries }, (_, i) => ((i * 37 + n.id * 11) % TRACKS.length) + 1),
  ]),
);
PLAYLISTS[3]?.splice(0, 0, { label: "---- OPEN ----" });
PLAYLISTS[3]?.splice(20, 0, { label: "──── Peak ────" });

function tracksIn(id: number): number[] {
  return (PLAYLISTS[id] ?? []).filter((e): e is number => typeof e === "number");
}

const smart = createMockSmart({
  nodes: NODES,
  staticTracks: tracksIn,
  tracks: TRACKS,
  hasAnyTag: tags.hasAny,
});

/** The playlist edits the UI can try out against the mock. */
function mutate(op: PlaylistOp) {
  const handled = smart.mutate(op);
  if (handled !== undefined) return handled;
  const done = { type: "done", createdId: null, added: 0, skipped: 0 } as const;
  if (!("playlistId" in op)) return done;
  const list = PLAYLISTS[op.playlistId] ?? [];
  switch (op.type) {
    case "insertDivider": {
      const label = op.label.trim();
      if (label === "") throw new Error("invalid divider label: must not be empty");
      list.splice(op.position ?? list.length, 0, { label });
      break;
    }
    case "renameDivider": {
      const label = op.label.trim();
      if (label === "") throw new Error("invalid divider label: must not be empty");
      if (typeof list[op.position] === "number") throw new Error("not a divider");
      list[op.position] = { label };
      break;
    }
    case "removeEntries": {
      const drop = new Set(op.positions);
      PLAYLISTS[op.playlistId] = list.filter((_, i) => !drop.has(i));
      break;
    }
    case "reorderEntries": {
      const moved = new Set(op.positions);
      const block = list.filter((_, i) => moved.has(i));
      const before = list.slice(0, op.before).filter((_, i) => !moved.has(i)).length;
      const rest = list.filter((_, i) => !moved.has(i));
      rest.splice(before, 0, ...block);
      PLAYLISTS[op.playlistId] = rest;
      break;
    }
    case "addTracks":
      list.splice(op.before ?? list.length, 0, ...op.trackIds);
      return { ...done, added: op.trackIds.length };
  }
  return done;
}

let settings: Settings = {
  audioDevice: null,
  keyNotation: "classical",
  deckWaveform: "threeBand",
  cacheDir: "/Users/dj/Library/Caches/reks",
  anlzPreference: "rekordboxFirst",
  onelibraryKeySet: false,
  ui: {
    hiddenColumns: null,
    filterBarOpen: true,
    filtersEnabled: true,
    infoOpen: false,
    volume: 0.8,
  },
};
let status: PlayerStatus = {
  loaded: false,
  playing: false,
  positionMs: 0,
  durationMs: 0,
  volume: 0.8,
  device: null,
  deviceLost: false,
  trackId: null,
};

const SORT: Partial<Record<SortColumn, keyof TrackRow>> = {
  title: "title",
  artist: "artist",
  album: "album",
  genre: "genre",
  label: "label",
  bpm: "bpmX100",
  key: "key",
  rating: "rating",
  color: "color",
  duration: "durationMs",
  year: "year",
  dateAdded: "dateAdded",
  playCount: "playCount",
  lastPlayed: "lastPlayed",
  kind: "kind",
  bitRate: "bitRateKbps",
  playlistPosition: "entryPosition",
};

function query(q: TrackQuery) {
  const playlist = q.filters.find((f) => f.type === "playlist");
  const smartList = playlist !== undefined && smart.isSmart(playlist.id);
  const entries = playlist === undefined ? [] : (PLAYLISTS[playlist.id] ?? []);
  let rows: TrackRow[] =
    playlist === undefined
      ? TRACKS
      : smartList
        ? smart.smartRows(playlist.id)
        : entries.flatMap((e, i) =>
            typeof e === "number" ? [{ ...(TRACKS[e - 1] as TrackRow), entryPosition: i }] : [],
          );
  const dividerView =
    !smartList &&
    q.rule === undefined &&
    playlist !== undefined &&
    PLAYLISTS[playlist.id] !== undefined &&
    q.sort.column === "playlistPosition" &&
    !q.sort.descending &&
    !q.text &&
    q.filters.length === 1;
  const dividers = dividerView
    ? entries.flatMap((e, position) =>
        typeof e === "number"
          ? []
          : [{ position, label: e.label, ascii: /^[ -~]*$/.test(e.label) }],
      )
    : [];
  const rule = q.rule;
  if (rule !== undefined) rows = rows.filter((r) => smart.matches(rule, r));
  const text = q.text?.toLowerCase();
  if (text) rows = rows.filter((r) => `${r.title} ${r.artist}`.toLowerCase().includes(text));
  for (const f of q.filters) {
    if (f.type === "genre") rows = rows.filter((r) => r.genre === f.genre);
    if (f.type === "keyIn") rows = rows.filter((r) => r.key !== null && f.keys.includes(r.key));
    if (f.type === "color") rows = rows.filter((r) => r.color === f.color);
    if (f.type === "ratingMin") rows = rows.filter((r) => r.rating >= f.rating);
    if (f.type === "bpmRange")
      rows = rows.filter((r) => r.bpmX100 !== null && r.bpmX100 >= f.min && r.bpmX100 <= f.max);
    if (f.type === "missing") rows = rows.filter((r) => r.missing === f.missing);
    if (f.type === "neverPlayed")
      rows = rows.filter((r) => r.playCount === 0 && r.lastPlayed === null);
    if (f.type === "playCountRange")
      rows = rows.filter((r) => r.playCount >= f.min && r.playCount <= f.max);
    if (f.type === "playedSince")
      rows = rows.filter((r) => r.lastPlayed !== null && r.lastPlayed >= f.date);
    if (f.type === "tagAny") rows = rows.filter((r) => tags.hasAny(r.id, f.ids));
  }
  // A smart list has no positions: its own sort stands in for "playlist order".
  const sort =
    smartList && playlist !== undefined && q.sort.column === "playlistPosition"
      ? {
          ...smart.sortOf(playlist.id),
          descending: smart.sortOf(playlist.id).descending !== q.sort.descending,
        }
      : q.sort;
  const col = SORT[sort.column] ?? "title";
  const dir = sort.descending ? -1 : 1;
  rows = [...rows].sort((a, b) => {
    const x = a[col] ?? "";
    const y = b[col] ?? "";
    return (x < y ? -1 : x > y ? 1 : 0) * dir;
  });
  return {
    rows: rows.slice(q.offset, q.offset + q.limit).map(tagged),
    total: rows.length,
    dividers,
    dividerView,
  };
}

function detail(id: number): TrackDetail {
  const row = tagged(TRACKS[id - 1] as TrackRow);
  return {
    track: {
      ...row,
      path: `/Users/dj/Music/${row.artist} - ${row.title}.${row.kind}`,
      fileSize: 12_000_000,
      fileMtime: "2026-01-01T00:00:00Z",
      albumArtist: null,
      composer: null,
      mix: null,
      grouping: null,
      trackNo: 1,
      discNo: null,
      sampleRate: 44100,
      bitDepth: 16,
      channels: 2,
      keySource: null,
      bpmSource: null,
      rbTrackId: null,
      createdAt: row.dateAdded,
      updatedAt: row.dateAdded,
      artwork: null,
    },
    cues: [
      { id: 1, slot: 0, kind: 0, startMs: 1200, endMs: null, name: "Intro", colorRgb: 0x30d158 },
      { id: 2, slot: 1, kind: 0, startMs: 64_000, endMs: null, name: "Drop", colorRgb: 0xff375f },
      { id: 3, slot: null, kind: 4, startMs: 96_000, endMs: 104_000, name: null, colorRgb: null },
    ],
    beatgrid: row.needsAnalysis
      ? null
      : {
          markers: 1,
          firstBeatMs: 120,
          bpmMinX100: row.bpmX100 ?? 0,
          bpmMaxX100: row.bpmX100 ?? 0,
        },
  };
}

function waveform(): ArrayBuffer {
  const n = 1200;
  const out = new Float32Array(n * 2);
  const r = rng(3);
  for (let i = 0; i < n; i++) {
    const env = 0.35 + 0.6 * Math.sin((i / n) * Math.PI) ** 0.6;
    const v = env * (0.6 + 0.4 * r());
    out[2 * i] = -v;
    out[2 * i + 1] = v;
  }
  return out.buffer;
}

const PREVIEW: ImportPreview = {
  total: 3112,
  playlists: 48,
  folders: 6,
  playlistEntries: 2210,
  withGrid: 2980,
  withCues: 2400,
  filesFound: 3090,
  filesNotFound: 22,
  noLocation: 0,
  notAFile: 0,
  invalidLocation: 0,
  inLibrary: 0,
  missingRoot: "/Volumes/OldDrive/Music",
  notFoundSample: [
    "/Volumes/OldDrive/Music/House/track01.mp3",
    "/Volumes/OldDrive/Music/House/track02.mp3",
  ],
};
const REPORT: ImportReport = {
  total: 3112,
  matchedByPath: 0,
  matchedByLocation: 0,
  matchedByFallback: 0,
  added: 3112,
  missing: 22,
  unmatched: [],
  warnings: [],
  beatgridRows: 2980,
  cueRows: 9100,
  playlistsCreated: 48,
  playlistsMatched: 0,
  playlistEntries: 2210,
  playlistEntriesSkipped: 0,
  tagsCreated: 4,
  tagsAssigned: 310,
  missingRoot: "/Volumes/OldDrive/Music",
  elapsedMs: 8400,
  cancelled: false,
};

let devices: DeviceInfo[] = [
  {
    mountPoint: "/Volumes/REKS",
    fs: { kind: "fat32" },
    fsName: "msdos",
    removable: true,
    totalBytes: 31_978_000_000,
    freeBytes: 18_402_000_000,
    label: "REKS",
    osName: "disk4s1",
  },
  {
    mountPoint: "/Volumes/GIGS",
    fs: { kind: "exFat" },
    fsName: "exfat",
    removable: true,
    totalBytes: 63_996_000_000,
    freeBytes: 61_120_000_000,
    label: "GIGS",
    osName: "disk5s1",
  },
  {
    mountPoint: "/Volumes/Backup",
    fs: { kind: "other", name: "apfs" },
    fsName: "apfs",
    removable: true,
    totalBytes: 500_000_000_000,
    freeBytes: 120_000_000_000,
    label: "Backup",
    osName: "disk6s1",
  },
];

/** Known sticks (stick profiles): REKS is "Main"; "Old gig stick" is not plugged in. */
let known: Device[] = [
  {
    id: 1,
    volumeSerial: "4993-1CFB",
    label: "REKS",
    sizeBytes: 31_978_000_000,
    name: "Main",
    target: "Cdj2000Nxs2",
    createdAt: "2026-10-01T10:00:00.000Z",
    lastSyncedAt: "2026-10-18T21:04:12.000Z",
  },
  {
    id: 2,
    volumeSerial: "7A21-0C3E",
    label: "OLDGIG",
    sizeBytes: 7_990_000_000,
    name: "Old gig stick",
    target: "Generic",
    createdAt: "2026-09-12T10:00:00.000Z",
    lastSyncedAt: "2026-09-12T18:30:00.000Z",
  },
];
const profiles: Record<number, number[]> = { 1: [1, 4], 2: [5] };
/** Playlists the last sync put on each stick. */
const onStick: Record<number, number[]> = { 1: [2, 3, 5], 2: [5] };

function identify(info: DeviceInfo): DeviceMatch {
  const device = known.find((d) => d.label === info.label && d.sizeBytes === info.totalBytes);
  if (device !== undefined) return { type: "known", device };
  if (info.label === "GIGS") {
    return {
      type: "foreign",
      volumeSerial: `${info.osName.toUpperCase()}-0001`,
      suggestedName: "Gigs",
      prompt: "This stick is new to reks.",
      note: "It holds a rekordbox library that reks did not write. Adopt keeps its playlists as library playlists and imports its play history (the stick is not changed). As a plain new stick, its playlists are replaced on the first sync (after the dry run and the history backup).",
    };
  }
  return {
    type: "new",
    volumeSerial: `${info.osName.toUpperCase()}-0001`,
    suggestedName: info.label || "USB stick",
    prompt: "This stick is new to reks.",
    note: "Name it and tick the playlists it should carry.",
  };
}

function mb(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${(bytes / 1e6).toFixed(1)} MB`;
}

/** Like `devices::profile_status`, with 9 MB per track. */
function profileStatus(id: number, mountPoint: string | null): ProfileStatus {
  const device = known.find((d) => d.id === id) as Device;
  const ticked = NODES.filter((n) => (profiles[id] ?? []).includes(n.id));
  const under = (n: PlaylistNode) => (n.isFolder ? NODES.filter((c) => c.parentId === n.id) : [n]);
  const covered = new Set(ticked.flatMap(under).map((n) => n.id));
  const path = (n: PlaylistNode) => {
    const parent = NODES.find((p) => p.id === n.parentId);
    return parent ? [parent.name, n.name] : [n.name];
  };
  const was = onStick[id] ?? [];
  const playlists: PlaylistStatus[] = NODES.filter((n) => covered.has(n.id)).map((n) => {
    const fresh = !was.includes(n.id);
    const state: PlaylistStatus["state"] = fresh
      ? { state: "new" }
      : n.id === 3
        ? { state: "changed", added: 3, removed: 1, reordered: false, movedFrom: null }
        : { state: "inSync" };
    const text = fresh
      ? "New on the stick at the next sync"
      : n.id === 3
        ? "3 tracks added, 1 removed"
        : "In sync";
    return {
      playlistId: n.id,
      path: path(n),
      pathText: path(n).join(" / "),
      tracks: n.entries,
      state,
      text,
    };
  });
  for (const n of NODES.filter((x) => was.includes(x.id) && !covered.has(x.id))) {
    playlists.push({
      playlistId: n.id,
      path: path(n),
      pathText: path(n).join(" / "),
      tracks: n.entries,
      state: { state: "willRemove" },
      text: "Removed from the stick at the next sync",
    });
  }
  const tracksOf = (n: PlaylistNode) => new Set(under(n).flatMap((c) => tracksIn(c.id)));
  const all = new Set(ticked.flatMap((n) => [...tracksOf(n)]));
  const entries = ticked.map((n) => {
    const others = new Set(ticked.filter((o) => o !== n).flatMap((o) => [...tracksOf(o)]));
    const unique = [...tracksOf(n)].filter((t) => !others.has(t)).length;
    return {
      playlistId: n.id,
      name: n.name,
      pathText: path(n).join(" / "),
      isFolder: n.isFolder,
      playlists: under(n).length,
      tracks: tracksOf(n).size,
      freesBytes: unique * 9e6,
      freesText:
        unique === 0
          ? "Unticking frees nothing: other ticked playlists hold all its tracks"
          : `Unticking frees ${mb(unique * 9e6)}`,
    };
  });
  const plannedBytes = all.size * 9e6;
  const counts = ["new", "changed", "willRemove"].map(
    (k) => playlists.filter((p) => p.state.state === k).length,
  );
  const parts = [
    counts[0] ? `${counts[0]} new` : "",
    counts[1] ? `${counts[1]} changed` : "",
    counts[2] ? `${counts[2]} to remove` : "",
  ].filter(Boolean);
  return {
    device,
    source: mountPoint === null ? "lastSync" : "stick",
    sourceText:
      mountPoint === null
        ? `As of the last sync, ${(device.lastSyncedAt ?? "").slice(0, 16).replace("T", " ")} UTC`
        : "Read from the stick",
    playlists,
    entries,
    plannedBytes,
    capacityBytes: device.sizeBytes,
    overCapacity: plannedBytes > device.sizeBytes,
    capacityText: `${mb(plannedBytes)} of ${mb(device.sizeBytes)}`,
    summary:
      playlists.length === 0 ? "No playlists in this profile" : parts.join(", ") || "In sync",
    notes: [],
  };
}

let planId = 0;
let lastPlan: { mountPoint: string; label: string; played: boolean; opts: ExportOptions } | null =
  null;
let cancelled = false;

/** Writes "12 tracks" in steps, then reports like `report::ReportView`. */
async function exportApply(onEvent: Channel<ExportProgress>): Promise<ExportReportView> {
  const plan = lastPlan;
  if (plan === null)
    throw new Error("this dry run is out of date or was already exported; plan again");
  lastPlan = null;
  cancelled = false;
  const send = (p: Partial<ExportProgress>) =>
    onEvent.onmessage({
      stage: "preflight",
      title: "Checking the stick",
      detail: null,
      done: 0,
      total: 0,
      cancellable: true,
      warnings: [],
      ...p,
    });
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  send({});
  await wait(400);
  if (plan.played && plan.opts.history === "import") {
    send({ stage: "importHistory", title: "Importing the stick's play history into your library" });
    await wait(400);
  }
  if (plan.played && plan.opts.history !== "discard") {
    send({ stage: "backup", title: "Backing up the stick's play history" });
    await wait(500);
  }
  const total = 12;
  for (let done = 1; done <= total && !cancelled; done++) {
    const t = TRACKS[done - 1] as TrackRow;
    send({
      stage: "write",
      title: "Writing tracks",
      detail: `Track ${done} of ${total}: ${t.artist ?? ""} – ${t.title ?? ""}`,
      done,
      total,
    });
    await wait(350);
  }
  const backupFolder = `/Users/dj/Library/Application Support/app.reks/stick-backups/4993-1CFB/2026-10-19T21-04-12Z`;
  const backedUp = plan.played && plan.opts.history !== "discard";
  const history = backedUp
    ? {
        title: "Play history backed up",
        text: `Before writing, reks copied 1 database file (412.0 KB) from the stick to the folder below and checked the copies. They keep the sessions players logged on this stick. ${cancelled ? "The stick still has them too: its database was not replaced." : "The stick's new database starts without history."}`,
        folder: backupFolder,
        files: ["/PIONEER/rekordbox/export.pdb"],
      }
    : plan.played
      ? {
          title: "Play history discarded",
          text: "As you chose, the stick's play history was erased without a backup.",
          folder: null,
          files: [],
        }
      : {
          title: "No play history on this stick",
          text: "Players had not logged any sessions here, so nothing was lost.",
          folder: null,
          files: [],
        };
  const base = { mountPoint: plan.mountPoint, deviceName: plan.label, history, warnings: [] };
  if (cancelled) {
    return {
      ...base,
      outcome: "cancelled",
      title: "Export cancelled",
      status:
        "The stick keeps its previous database. Audio and analysis copied before you cancelled stay on the stick; the old database doesn't list them.",
      facts: [],
      problems: [],
    };
  }
  if (plan.opts.onelibrary) {
    send({
      stage: "oneLibrary",
      title: "Writing exportLibrary.db (experimental)",
      cancellable: false,
    });
    await wait(400);
  }
  for (const [stage, title] of [
    ["sync", "Flushing the written files to the stick"],
    ["validate", "Reading the stick back to check it"],
    ["manifest", "Recording what is on the stick"],
  ] as const) {
    send({ stage, title, cancellable: false });
    await wait(400);
  }
  return {
    ...base,
    outcome: "success",
    title: "Export finished",
    status:
      "reks wrote the stick and read it back: it matches the plan. Eject it before unplugging.",
    facts: [
      { label: "Tracks", value: "12 in the stick's database" },
      { label: "Audio", value: "12 copied, 0 already on the stick" },
      { label: "Analysis", value: "36 files written, 0 unchanged" },
      ...(plan.opts.onelibrary
        ? [
            {
              label: "OneLibrary (experimental)",
              value: "exportLibrary.db written: 12 tracks, 122.0 KB",
            },
          ]
        : []),
      { label: "Read back", value: "12 tracks and 2 playlists and folders, as planned" },
      { label: "Library record", value: "12 tracks recorded as on this stick" },
    ],
    problems: [],
  };
}

/** A dry run in the shape `dryrun::DryRun` gives: GIGS holds play history, Backup is blocked. */
function dryRun(mountPoint: string, sel: ExportSelection, opts: ExportOptions): DryRun {
  const dev = devices.find((d) => d.mountPoint === mountPoint) ?? (devices[0] as DeviceInfo);
  const lists = NODES.filter((n) => sel.playlists.includes(n.id));
  const copies = lists.slice(0, 1).flatMap(() =>
    TRACKS.slice(0, 12).map((t) => ({
      path: `/Contents/${t.artist ?? "Unknown Artist"}/${t.album ?? "Unknown Album"}/${t.title}.mp3`,
      from: `/Users/dj/Music/${t.title}.mp3`,
      sizeText: "9.4 MB",
    })),
  );
  const played = dev.label === "GIGS";
  const problems: DryRun["problems"] = [];
  if (dev.fs.kind === "other") {
    problems.push({
      kind: "unsupportedFilesystem",
      title: `Players cannot read the ${dev.fsName} file system; format the stick as FAT32.`,
      items: [],
    });
  }
  if (played && !opts.historyChosen) {
    problems.push({
      kind: "historyNotImported",
      title:
        "The stick holds 2 play history sessions not imported into your library. Choose what happens to them under the play history.",
      items: [],
    });
  }
  if (played && opts.history === "discard" && !opts.discardHistoryConfirmed) {
    problems.push({
      kind: "discardNotConfirmed",
      title: "Discarding the stick's play history needs your confirmation.",
      items: [],
    });
  }
  // The OneLibrary step (phase 4 task 8): blocked only without the key.
  if (opts.onelibrary && !settings.onelibraryKeySet) {
    problems.push({
      kind: "oneLibraryKeyMissing",
      title:
        "The OneLibrary export needs its key: paste it in Settings → Experimental or set REKS_ONELIBRARY_KEY.",
      items: [],
    });
  }
  const none = { count: 0, bytes: 0, totalText: "None", files: [] };
  const write = copies.length * 9_400_000;
  lastPlan = { mountPoint: dev.mountPoint, label: dev.label, played, opts };
  return {
    planId: ++planId,
    ready: problems.length === 0,
    status:
      problems.length === 0
        ? "Ready to export. Nothing has been written to the stick yet."
        : `Blocked by ${problems.length} problem${problems.length === 1 ? "" : "s"}. Fix ${problems.length === 1 ? "it" : "them"} or change the options; nothing has been written.`,
    device: {
      mountPoint: dev.mountPoint,
      name: dev.label,
      fsText: `${dev.fs.kind === "fat32" ? "FAT32" : dev.fsName} (${dev.fsName})`,
      spaceText: `${(dev.freeBytes / 1e9).toFixed(1)} GB free of ${(dev.totalBytes / 1e9).toFixed(1)} GB`,
      volumeSerial: "4993-1CFB",
    },
    target: opts.target === "Generic" ? "Any player" : opts.target,
    summary: [
      {
        label: "Tracks",
        value: `${copies.length}: ${copies.length} to copy, 0 already on the stick`,
      },
      ...(opts.dividers
        ? [
            {
              label: "Dividers",
              value:
                "2 dividers, 1.4 MB: silent tracks titled with the label, analysis generated (UNVERIFIED on players)",
            },
          ]
        : []),
      { label: "Playlists", value: String(lists.length) },
      { label: "Analysis", value: `${copies.length - 1} from rekordbox (1.2 MB)` },
      { label: "Copy", value: `${copies.length} files, ${(write / 1e6).toFixed(1)} MB` },
      { label: "Delete", value: "None" },
    ],
    playlists: lists.map((p) => ({
      id: p.id,
      name: p.name,
      depth: 0,
      isFolder: p.isFolder,
      smart: p.smart,
      detail: p.isFolder ? "" : `${p.entries} tracks`,
    })),
    copies: {
      count: copies.length,
      bytes: write,
      totalText: `${copies.length} files, ${(write / 1e6).toFixed(1)} MB`,
      files: copies,
    },
    deletes: none,
    space: {
      fits: true,
      totalBytes: dev.totalBytes,
      freeBytes: dev.freeBytes,
      writeBytes: write,
      deleteBytes: 0,
      freeAfterBytes: dev.freeBytes - write,
      usedAfterBytes: dev.totalBytes - dev.freeBytes + write,
      facts: [
        { label: "Free now", value: `${(dev.freeBytes / 1e9).toFixed(1)} GB` },
        { label: "Writes", value: `${(write / 1e6).toFixed(1)} MB` },
        { label: "Deletes free", value: "0 B" },
        { label: "Free after", value: `${((dev.freeBytes - write) / 1e9).toFixed(1)} GB` },
      ],
    },
    history: played
      ? {
          atRisk: true,
          policy: opts.history,
          title: "This stick holds play history",
          text: "The stick's export.pdb holds 2 history sessions with 31 played tracks. Exporting rebuilds the stick's database, which erases it.",
          guard: {
            title: "2 sessions on this stick are not in your library",
            text: "Exporting erases them from the stick. Import now adds them to your library's history before writing; Back up and continue keeps a copy you can import later; Continue and discard erases them. Imported sessions count in play counts; you can mark any of them as practice in History later.",
            sessions: ["HISTORY 001: 19 plays", "HISTORY 002: 12 plays"],
            chosen: opts.historyChosen,
            canImport: true,
          },
          actionText:
            opts.history === "import"
              ? "Before writing anything, reks imports 2 sessions (31 plays) into your library, then reks copies 1 database file (412.0 KB) to its data folder and checks the copies; if the backup fails, nothing is written. The report after the export shows where the copies are."
              : opts.history === "backUp"
                ? "Before writing anything, reks copies 1 database file (412.0 KB) to its data folder and checks the copies; if the backup fails, nothing is written. The report after the export shows where the copies are."
                : opts.discardHistoryConfirmed
                  ? "You chose to discard it: no backup is made."
                  : "Discarding it needs your confirmation in the options.",
          backups:
            opts.history !== "discard"
              ? {
                  count: 1,
                  bytes: 412_000,
                  totalText: "1 file, 412.0 KB",
                  files: [
                    { path: "/PIONEER/rekordbox/export.pdb", from: null, sizeText: "412.0 KB" },
                  ],
                }
              : none,
        }
      : {
          atRisk: false,
          policy: opts.history,
          title: "No play history on this stick",
          text: "Players have not logged any sessions here, so the export loses nothing.",
          actionText: null,
          backups: none,
          guard: null,
        },
    problems,
    warnings: [
      ...(lists.some((p) => p.smart)
        ? [
            {
              kind: "smartPlaylistFixed",
              title: `${lists.filter((p) => p.smart).length === 1 ? "1 smart playlist is" : `${lists.filter((p) => p.smart).length} smart playlists are`} written with the tracks matched now. Players have no smart playlists, so the stick won't update by itself.`,
              items: lists.filter((p) => p.smart).map((p) => p.name),
            },
          ]
        : []),
      ...(copies.length > 0
        ? [
            {
              kind: "skipped",
              title: "1 track left out",
              items: [`${copies[0]?.path ?? ""}: never analysed by rekordbox`],
            },
          ]
        : []),
      ...(opts.onelibrary
        ? [
            {
              kind: "oneLibraryExperimental",
              title:
                "OneLibrary (exportLibrary.db) is experimental: no player has been tested with reks's file, and AlphaTheta does not support third-party OneLibrary exports.",
              items: [],
            },
          ]
        : []),
    ],
    onelibrary:
      opts.onelibrary && settings.onelibraryKeySet
        ? {
            title: "OneLibrary (experimental)",
            text: "Also writes exportLibrary.db for players that read OneLibrary, next to the Device Library. No player has been tested with it, and AlphaTheta does not support it. It replaces the exportLibrary.db already on the stick.",
            write: {
              path: "/PIONEER/rekordbox/exportLibrary.db",
              from: null,
              sizeText: "size not known yet",
            },
            removes: {
              count: 2,
              bytes: 1_200_000,
              totalText: "2 files, 1.2 MB",
              files: [
                { path: "/PIONEER/rekordbox/exportLibrary.db-wal", from: null, sizeText: "1.1 MB" },
                { path: "/PIONEER/rekordbox/exportExt.pdb", from: null, sizeText: "98.3 KB" },
              ],
            },
          }
        : null,
    convertible: copies.length > 0 ? TRACKS.slice(0, 2).map((t) => t.id) : [],
    convertText:
      copies.length > 0
        ? "2 tracks can be converted to a format older players load (AIFF, up to 24-bit / 48 kHz). The library track keeps its cues, grid and playlists."
        : null,
  };
}

let lastConvert: ConvertDryRun | null = null;
let convertCancelled = false;

/** Like `convert::plan_view`: every third track is an MP3 and is skipped. */
function convertDryRun(tracks: number[], spec: ConvertSpec, opts: ConvertOptions): ConvertDryRun {
  const rows = tracks.map((id) => TRACKS.find((t) => t.id === id)).filter((t) => t !== undefined);
  const label = (t: TrackRow) => `${t.artist ?? ""} – ${t.title ?? ""}`;
  const ext = spec.target;
  const items = rows
    .filter((t) => t.id % 3 !== 0)
    .map((t) => ({
      trackId: t.id,
      label: label(t),
      source: `/Users/dj/Music/${t.title}.flac`,
      dest: `${typeof opts.destination === "string" ? "/Users/dj/Music" : opts.destination.folder}/${t.title}.${ext}`,
      from: "FLAC · 24-bit · 96 kHz · stereo",
      to: `${ext.toUpperCase()} · 24-bit · ${opts.capRate48k || spec.rate === "hz48000" ? "48 kHz" : spec.rate === "hz44100" ? "44.1 kHz" : "96 kHz"} · stereo`,
      sizeText: "41.2 MB → 23.0 MB",
    }));
  lastConvert = {
    planId: ++planId,
    ready: items.length > 0,
    status:
      items.length > 0
        ? `Ready to convert ${items.length} file${items.length === 1 ? "" : "s"}. Nothing has been written yet.`
        : "Nothing to convert. Nothing has been written.",
    facts: [
      {
        label: "Files",
        value: `${items.length} to convert, ${rows.length - items.length} skipped`,
      },
      {
        label: "Size",
        value: `${(items.length * 41.2).toFixed(1)} MB → about ${(items.length * 23).toFixed(1)} MB`,
      },
    ],
    items,
    skipped: rows
      .filter((t) => t.id % 3 === 0)
      .map((t) => ({
        trackId: t.id,
        label: label(t),
        reason: "MP3 or AAC: only lossless files convert",
      })),
    warnings: [],
  };
  return lastConvert;
}

async function convertApplyMock(onEvent: Channel<ConvertEvent>): Promise<ConvertReportView> {
  const plan = lastConvert;
  if (plan === null)
    throw new Error("this dry run is out of date or was already converted; plan again");
  lastConvert = null;
  convertCancelled = false;
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const done: ConvertReportView["converted"] = [];
  for (const [index, it] of plan.items.entries()) {
    if (convertCancelled) break;
    onEvent.onmessage({ type: "started", trackId: it.trackId, index, total: plan.items.length });
    for (let f = 1; f <= 4; f++) {
      await wait(250);
      onEvent.onmessage({ type: "progress", trackId: it.trackId, fraction: f / 4 });
    }
    done.push({
      trackId: it.trackId,
      newTrackId: it.trackId,
      label: it.label,
      dest: it.dest,
      original: `Original kept: ${it.source}`,
    });
  }
  return {
    outcome: convertCancelled ? "cancelled" : "success",
    title: convertCancelled ? "Conversion cancelled" : "Conversion finished",
    status: `Converted ${done.length} of ${plan.items.length} files.${convertCancelled ? " Files not started are unchanged." : ""}`,
    facts: [
      {
        label: "Size",
        value: `${(done.length * 41.2).toFixed(1)} MB → ${(done.length * 23).toFixed(1)} MB`,
      },
    ],
    converted: done,
    failed: [],
    skipped: plan.skipped,
  };
}

mockIPC((cmd, payload) => {
  const args = (payload ?? {}) as Record<string, unknown>;
  switch (cmd) {
    case "app_ready":
      return null;
    case "library_query":
      return query(args.query as TrackQuery);
    case "analysis_subscribe":
    case "analysis_pause":
    case "analysis_resume":
    case "analysis_cancel":
      return {
        queued: 0,
        running: 0,
        done: 0,
        failed: 0,
        total: 0,
        paused: cmd === "analysis_pause",
      };
    case "analysis_reanalyse":
      return (args.ids as number[]).length;
    case "library_facets":
      smart.refreshCounts();
      return { genres: GENRES, keys: KEYS, colours: COLOURS, playlists: NODES } satisfies Facets;
    case "library_track":
    case "library_update_track":
      return detail(args.id as number);
    case "artwork_get":
      throw new Error("no artwork in the mock");
    case "playlists_get":
      smart.refreshCounts();
      return { nodes: NODES };
    case "playlist_rule":
      return smart.ruleOf(args.id as number);
    case "playlists_mutate":
      return mutate(args.op as PlaylistOp);
    case "tags_get":
      return tags.tree();
    case "tags_mutate":
      return tags.mutate(args.op as TagOp);
    case "settings_get":
      return settings;
    case "settings_set": {
      // Like Rust: the key is stored, never echoed back.
      const { onelibraryKey, ...patch } = args.patch as SettingsPatch;
      settings = { ...settings, ...patch };
      if (onelibraryKey !== undefined) settings.onelibraryKeySet = onelibraryKey !== null;
      return settings;
    }
    case "library_roots":
      return [{ id: 1, path: "/Users/dj/Music", lastScan: "2026-10-18T19:48:00Z" }];
    case "player_devices":
      return ["MacBook Pro Speakers", "USB Audio Interface"];
    case "player_state":
      return status;
    case "player_load": {
      const row = TRACKS[(args.id as number) - 1] as TrackRow;
      status = {
        ...status,
        loaded: true,
        playing: args.play as boolean,
        trackId: row.id,
        positionMs: 0,
        durationMs: row.durationMs ?? 0,
      };
      return status;
    }
    case "player_play":
      status = { ...status, playing: true };
      return status;
    case "player_pause":
      status = { ...status, playing: false };
      return status;
    case "player_seek":
      status = { ...status, positionMs: args.ms as number };
      return status;
    case "player_volume":
      status = { ...status, volume: args.volume as number };
      return status;
    case "player_set_device":
      status = { ...status, device: args.device as string | null };
      return status;
    case "overview_waveform":
      return waveform();
    case "rbxml_preview":
      return PREVIEW;
    case "rbxml_import_start":
      return REPORT;
    case "devices_list":
      return devices;
    case "device_eject":
      devices = devices.filter((d) => d.mountPoint !== args.mountPoint);
      return null;
    case "profiles_list":
      return known;
    case "profile_identify":
      return identify(args.info as DeviceInfo);
    case "profile_create": {
      const info = args.info as DeviceInfo;
      const device: Device = {
        id: known.length + 1,
        volumeSerial: args.volumeSerial as string | null,
        label: info.label,
        sizeBytes: info.totalBytes,
        name: args.name as string,
        target: args.target as Device["target"],
        createdAt: new Date().toISOString(),
        lastSyncedAt: null,
      };
      known = [...known, device];
      return device;
    }
    case "profile_selection":
      return { playlists: profiles[args.id as number] ?? [] };
    case "profile_set_selection":
      profiles[args.id as number] = args.playlists as number[];
      return null;
    case "profile_set_target":
      known = known.map((d) =>
        d.id === args.id ? { ...d, target: args.target as Device["target"] } : d,
      );
      return known.find((d) => d.id === args.id);
    case "profile_status":
      return profileStatus(args.id as number, args.mountPoint as string | null);
    case "export_plan":
      return new Promise((resolve) =>
        setTimeout(
          () =>
            resolve(
              dryRun(
                args.mountPoint as string,
                args.selection as ExportSelection,
                args.options as ExportOptions,
              ),
            ),
          600,
        ),
      );
    case "export_apply":
      return exportApply(args.onEvent as Channel<ExportProgress>);
    case "export_cancel":
      cancelled = true;
      return null;
    case "convert_plan":
      return new Promise((resolve) =>
        setTimeout(
          () =>
            resolve(
              convertDryRun(
                args.tracks as number[],
                args.spec as ConvertSpec,
                args.options as ConvertOptions,
              ),
            ),
          300,
        ),
      );
    case "convert_apply":
      return convertApplyMock(args.onEvent as Channel<ConvertEvent>);
    case "convert_cancel":
      convertCancelled = true;
      return null;
    case "plugin:dialog|open":
      return "/Users/dj/Music/rekordbox/collection.xml";
    default:
      return (
        history(cmd, args) ??
        mockAdopt(cmd, args) ??
        mockGrid(cmd, args, (id) => TRACKS[id - 1]?.needsAnalysis === false) ??
        mockMix(cmd, args, (id) => TRACKS[id - 1]?.needsAnalysis === false) ??
        null
      );
  }
});
