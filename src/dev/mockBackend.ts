/**
 * Dev-only stand-in for the Rust commands, so the UI runs in a plain browser
 * (`npm run dev`, then open http://localhost:1420): generated tracks,
 * playlists, settings and an import preview. Loaded by `main.tsx` only in
 * dev builds outside Tauri; never part of a release bundle.
 */
import { mockIPC } from "@tauri-apps/api/mocks";

import type {
  DeviceInfo,
  Facets,
  ImportPreview,
  ImportReport,
  KeyInfo,
  PlayerStatus,
  PlaylistNode,
  Settings,
  SortColumn,
  TrackDetail,
  TrackQuery,
  TrackRow,
} from "../lib/ipc";

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
  playCount: Math.floor(rand() * 40),
  missing: rand() < 0.02,
  needsAnalysis: rand() < 0.08,
  entryPosition: null,
}));

const NODES: PlaylistNode[] = [
  { id: 1, parentId: null, name: "Gigs", isFolder: true, depth: 0, position: 0, entries: 0 },
  { id: 2, parentId: 1, name: "Warm-up", isFolder: false, depth: 1, position: 0, entries: 40 },
  { id: 3, parentId: 1, name: "Peak time", isFolder: false, depth: 1, position: 1, entries: 55 },
  {
    id: 4,
    parentId: null,
    name: "Digging 2026",
    isFolder: false,
    depth: 0,
    position: 1,
    entries: 120,
  },
  { id: 5, parentId: null, name: "Closing", isFolder: false, depth: 0, position: 2, entries: 25 },
];
const PLAYLISTS: Record<number, number[]> = Object.fromEntries(
  NODES.filter((n) => !n.isFolder).map((n) => [
    n.id,
    Array.from({ length: n.entries }, (_, i) => ((i * 37 + n.id * 11) % TRACKS.length) + 1),
  ]),
);

let settings: Settings = {
  audioDevice: null,
  keyNotation: "classical",
  cacheDir: "/Users/dj/Library/Caches/reks",
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
  kind: "kind",
  bitRate: "bitRateKbps",
  playlistPosition: "entryPosition",
};

function query(q: TrackQuery) {
  const playlist = q.filters.find((f) => f.type === "playlist");
  let rows: TrackRow[] =
    playlist === undefined
      ? TRACKS
      : (PLAYLISTS[playlist.id] ?? []).map((id, i) => ({
          ...(TRACKS[id - 1] as TrackRow),
          entryPosition: i,
        }));
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
    if (f.type === "needsAnalysis") rows = rows.filter((r) => r.needsAnalysis === f.needs);
  }
  const col = SORT[q.sort.column] ?? "title";
  const dir = q.sort.descending ? -1 : 1;
  rows = [...rows].sort((a, b) => {
    const x = a[col] ?? "";
    const y = b[col] ?? "";
    return (x < y ? -1 : x > y ? 1 : 0) * dir;
  });
  return { rows: rows.slice(q.offset, q.offset + q.limit), total: rows.length };
}

function detail(id: number): TrackDetail {
  const row = TRACKS[id - 1] as TrackRow;
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
      lastPlayed: null,
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

mockIPC((cmd, payload) => {
  const args = (payload ?? {}) as Record<string, unknown>;
  switch (cmd) {
    case "library_query":
      return query(args.query as TrackQuery);
    case "library_facets":
      return { genres: GENRES, keys: KEYS, colours: COLOURS, playlists: NODES } satisfies Facets;
    case "library_track":
    case "library_update_track":
      return detail(args.id as number);
    case "artwork_get":
      throw new Error("no artwork in the mock");
    case "playlists_get":
      return { nodes: NODES };
    case "playlists_mutate":
      return { type: "done", createdId: null, added: 0, skipped: 0 };
    case "settings_get":
      return settings;
    case "settings_set":
      settings = { ...settings, ...(args.patch as Partial<Settings>) };
      return settings;
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
    case "plugin:dialog|open":
      return "/Users/dj/Music/rekordbox/collection.xml";
    default:
      return null;
  }
});
