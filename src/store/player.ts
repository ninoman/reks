/**
 * Preview player (Zustand): mirrors `player_state` from Rust and forwards
 * transport actions. Playback, timing and decoding all live in reks-mix /
 * reks-audio; this polls the state while playing and keeps the overview
 * waveform of the loaded track.
 */
import { create } from "zustand";

import {
  overviewWaveform,
  type PlayerStatus,
  playerDevices,
  playerLoad,
  playerPause,
  playerPlay,
  playerSeek,
  playerSetDevice,
  playerState,
  playerVolume,
  type TrackRow,
} from "../lib/ipc";
import { overviewColumns } from "../lib/overview";

/** Poll period while playing (docs/phases/phase-1.md: `player_state` every 100 ms). */
export const POLL_MS = 100;

export const idleStatus: PlayerStatus = {
  loaded: false,
  playing: false,
  positionMs: 0,
  durationMs: 0,
  volume: 1,
  device: null,
  deviceLost: false,
  trackId: null,
};

/** What the transport shows for the loaded track. */
export interface LoadedTrack {
  id: number;
  title: string | null;
  artist: string | null;
}

export interface PlayerStore {
  status: PlayerStatus;
  track: LoadedTrack | null;
  /** Track id the overview belongs to, and its `[min, max]` columns. */
  overview: { trackId: number; columns: Float32Array } | null;
  /** Track id being decoded, if any. */
  loading: number | null;
  devices: string[];
  error: string | null;

  load: (row: Pick<TrackRow, "id" | "title" | "artist">, play?: boolean) => Promise<void>;
  toggle: () => Promise<void>;
  seek: (ms: number) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  setDevice: (device: string | null) => Promise<void>;
  refreshDevices: () => Promise<void>;
  poll: () => Promise<void>;
}

let timer: ReturnType<typeof setInterval> | null = null;
/** Newest seek not sent yet; a scrub sends only the latest target. */
let seekTarget: number | null = null;
let seeking = false;
/** Bumped by every action; a poll sent before the latest one answers stale. */
let actions = 0;

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export const usePlayer = create<PlayerStore>((set, get) => {
  /** Applies a status from Rust and starts or stops polling to match. */
  function apply(status: PlayerStatus) {
    set((s) => ({
      status,
      track: status.trackId === null ? null : s.track,
      overview: status.trackId === null ? null : s.overview,
      error: null,
    }));
    if (status.playing && timer === null) {
      timer = setInterval(() => void get().poll(), POLL_MS);
    } else if (!status.playing && timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  async function run(action: () => Promise<PlayerStatus>, poll = false) {
    const seq = poll ? actions : ++actions;
    try {
      const status = await action();
      if (poll && seq !== actions) return;
      apply(status);
    } catch (e) {
      set({ error: message(e) });
    }
  }

  return {
    status: idleStatus,
    track: null,
    overview: null,
    loading: null,
    devices: [],
    error: null,

    async load(row, play = true) {
      // Re-activating the track that is already playing keeps it going.
      const { status } = get();
      if (play && status.playing && status.trackId === row.id) return;
      set({ loading: row.id, error: null });
      try {
        const status = await playerLoad(row.id, play);
        // A newer load won in Rust: keep showing what is actually loaded.
        if (status.trackId === row.id) {
          set({ track: { id: row.id, title: row.title, artist: row.artist } });
        }
        apply(status);
        if (status.trackId === row.id && get().overview?.trackId !== row.id) {
          const columns = overviewColumns(await overviewWaveform(row.id));
          if (get().status.trackId === row.id) set({ overview: { trackId: row.id, columns } });
        }
      } catch (e) {
        set({ error: message(e) });
      } finally {
        if (get().loading === row.id) set({ loading: null });
      }
    },

    async toggle() {
      const { status } = get();
      if (!status.loaded) return;
      // Flip the button now, so a second press right after sends the opposite.
      set({ status: { ...status, playing: !status.playing } });
      await run(status.playing ? playerPause : playerPlay);
    },

    async seek(ms) {
      const { status } = get();
      if (!status.loaded) return;
      const target = Math.min(status.durationMs, Math.max(0, ms));
      // Show the jump now; Rust reports the target until the deck takes it.
      set({ status: { ...status, positionMs: target } });
      seekTarget = target;
      if (seeking) return;
      seeking = true;
      try {
        while (seekTarget !== null) {
          const next = seekTarget;
          seekTarget = null;
          await run(() => playerSeek(next));
        }
      } finally {
        seeking = false;
      }
    },
    setVolume: (volume) => run(() => playerVolume(volume)),
    setDevice: (device) => run(() => playerSetDevice(device)),

    async refreshDevices() {
      try {
        set({ devices: await playerDevices() });
      } catch (e) {
        set({ error: message(e) });
      }
    },

    poll: () => run(playerState, true),
  };
});
