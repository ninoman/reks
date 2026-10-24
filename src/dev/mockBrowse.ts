/**
 * Dev-only stick browsing for the mock backend (`mockBackend.ts`): the REKS
 * stick holds a rekordbox library built from some of the mock tracks (a
 * folder, playlists and two player histories, one naming a track the stick
 * no longer has); other drives have none. Grids and waveforms are the mock
 * fixture's, as for analysed library tracks (`mockGrid.ts`); playing one is
 * `mix_load_stick` in `mockMix.ts`. `device_check` reports warnings and a
 * problem on the REKS stick and no library elsewhere, in Rust's wording.
 */

import type { DeviceInfo, StickLibrary, StickReport, StickTrack, TrackRow } from "../lib/ipc";

const STICK = "/Volumes/REKS";

function stickTrack(t: TrackRow, id: number): StickTrack {
  return {
    id,
    title: t.title ?? "Untitled",
    artist: t.artist,
    album: t.album,
    genre: t.genre,
    bpmX100: t.bpmX100,
    key: t.key,
    durationMs: t.durationMs,
    stickPath: `/Contents/${t.artist ?? "UnknownArtist"}/${t.title ?? "Untitled"}.mp3`,
  };
}

function library(tracks: readonly TrackRow[]): StickLibrary {
  const onStick = tracks.slice(0, 40).map((t, i) => stickTrack(t, i + 1));
  const ids = (from: number, n: number) => onStick.slice(from, from + n).map((t) => t.id);
  return {
    tracks: onStick,
    playlists: [
      { id: 1, parentId: null, name: "Gigs", isFolder: true, trackIds: [] },
      { id: 2, parentId: null, name: "Warm-up", isFolder: false, trackIds: ids(0, 14) },
      { id: 3, parentId: 1, name: "Friday at Tresor", isFolder: false, trackIds: ids(10, 18) },
      { id: 4, parentId: 1, name: "Closing", isFolder: false, trackIds: ids(26, 12) },
    ],
    histories: [
      { id: 1, name: "HISTORY 001", trackIds: ids(2, 9) },
      { id: 2, name: "HISTORY 002", trackIds: [...ids(20, 6), 999, ...ids(30, 4)] },
    ],
  };
}

const DEVICE_LIBRARY_PLAYERS = [
  "CDJ-3000",
  "CDJ-2000NXS2 and older CDJ-2000 models",
  "CDJ-900NXS2",
  "XDJ-XZ",
  "XDJ-RX3",
  "XDJ-RR",
  "XDJ-1000MK2",
  "XDJ-700",
];
const ONELIBRARY_PLAYERS = ["CDJ-3000X", "CDJ-1500X", "OPUS-QUAD", "OMNIS-DUO", "XDJ-AZ", "XDJ-AN"];

function check(info: DeviceInfo, tracks: readonly TrackRow[]): StickReport {
  const has = info.mountPoint === STICK;
  const lib = library(tracks);
  const track = (t: StickTrack) => ({
    subject: t.artist === null ? t.title : `${t.artist} – ${t.title}`,
    detail: null,
    path: t.stickPath,
  });
  const formats: StickReport["formats"] = [
    {
      format: "deviceLibrary",
      name: "Device Library",
      file: "/PIONEER/rekordbox/export.pdb",
      present: has,
      status: has ? `${lib.tracks.length} tracks in 3 playlists` : "Not on this drive",
      players: DEVICE_LIBRARY_PLAYERS,
      playersVerified: false,
      note: "The classic rekordbox stick library. Players that read it ignore exportLibrary.db.",
    },
    {
      format: "deviceLibraryExt",
      name: "Device Library extension",
      file: "/PIONEER/rekordbox/exportExt.pdb",
      present: false,
      status: "Not on this drive",
      players: [],
      playersVerified: false,
      note: "Extra data rekordbox writes beside export.pdb. reks does not write it; the notice names no player that needs it.",
    },
    {
      format: "oneLibrary",
      name: "OneLibrary",
      file: "/PIONEER/rekordbox/exportLibrary.db",
      present: false,
      status: "Not on this drive",
      players: ONELIBRARY_PLAYERS,
      playersVerified: false,
      note: "The newer encrypted library. Players that read it ignore export.pdb.",
    },
  ];
  const sections: StickReport["sections"] = has
    ? [
        {
          section: "library",
          title: "Libraries",
          groups: [
            {
              kind: "noOneLibrary",
              severity: "warning",
              title: "No OneLibrary (exportLibrary.db)",
              detail:
                "Players that read only exportLibrary.db (CDJ-3000X, OPUS-QUAD, XDJ-AZ, OMNIS-DUO and others) will show no playlists from this stick.",
              items: [],
            },
          ],
        },
        {
          section: "files",
          title: "Track files",
          groups: [
            {
              kind: "audioMissing",
              severity: "problem",
              title: "1 track: audio file missing",
              detail: "Players list these tracks but can't load them.",
              items: lib.tracks.slice(5, 6).map(track),
            },
            {
              kind: "noWaveformPreview",
              severity: "warning",
              title: "2 tracks without a waveform preview",
              detail:
                "After rekordbox 7.2.12 wrote sticks players could not load, AlphaTheta told users to look for tracks like these.",
              items: lib.tracks.slice(8, 10).map(track),
            },
          ],
        },
        {
          section: "history",
          title: "Play history",
          groups: [
            {
              kind: "historyNotImported",
              severity: "note",
              title: "2 play history sessions not imported into your library",
              detail:
                "Import them (Histories → +) so their plays count. reks asks before an export would replace them.",
              items: lib.histories.map((h) => ({
                subject: h.name,
                detail: `Device Library · ${h.trackIds.length} tracks`,
                path: null,
              })),
            },
          ],
        },
      ]
    : [
        {
          section: "library",
          title: "Libraries",
          groups: [
            {
              kind: "noLibrary",
              severity: "problem",
              title: "No DJ library on this drive",
              detail:
                "There is neither export.pdb nor exportLibrary.db, so players can only browse its folders, without waveforms, beatgrids or cues.",
              items: [],
            },
          ],
        },
      ];
  return {
    mountPoint: info.mountPoint,
    label: info.label,
    target: "Cdj2000Nxs",
    targetText:
      "Audio checked against CDJ-2000NXS, the strictest players. Pick the oldest player the stick must work on in Export to USB to check against it.",
    summary: has
      ? {
          status: "problems",
          problems: 1,
          warnings: 3,
          notes: 2,
          headline: "1 problem, 3 warnings",
        }
      : { status: "problems", problems: 1, warnings: 0, notes: 0, headline: "1 problem" },
    formats,
    playersSource:
      "AlphaTheta notice of 2026-03-18 and the rekordbox FAQ. UNVERIFIED: reks has not tested any of these players.",
    filesystem: {
      fs: info.fs,
      fsName: info.fsName,
      totalBytes: info.totalBytes,
      freeBytes: info.freeBytes,
      text: "FAT32 · 3.2 GB free of 16.0 GB",
    },
    tracksChecked: has ? lib.tracks.length : 0,
    sections,
  };
}

export function mockBrowse(
  cmd: string,
  args: Record<string, unknown>,
  tracks: readonly TrackRow[],
): unknown {
  switch (cmd) {
    case "device_library":
      // Reading a stick takes a moment.
      return new Promise((resolve) =>
        setTimeout(() => resolve(args.mountPoint === STICK ? library(tracks) : null), 400),
      );
    case "device_check":
      // Checking reads every track's headers: slower than browsing.
      return new Promise((resolve) =>
        setTimeout(() => resolve(check(args.info as DeviceInfo, tracks)), 1200),
      );
    default:
      return undefined;
  }
}
