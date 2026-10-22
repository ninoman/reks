/**
 * Dev-only stick browsing for the mock backend (`mockBackend.ts`): the REKS
 * stick holds a rekordbox library built from some of the mock tracks (a
 * folder, playlists and two player histories, one naming a track the stick
 * no longer has); other drives have none. Grids and waveforms are the mock
 * fixture's, as for analysed library tracks (`mockGrid.ts`); playing one is
 * `mix_load_stick` in `mockMix.ts`.
 */

import type { StickLibrary, StickTrack, TrackRow } from "../lib/ipc";

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
    default:
      return undefined;
  }
}
