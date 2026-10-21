/**
 * Mixer (Zustand): starts and stops the `reks-mix` engine, mirrors the state
 * it streams (about 60 a second), loads library tracks onto decks with their
 * waveform and beat lines, and keeps the mixer's knob and switch positions
 * and the player's volume, which it sends as `MixControl`s. The library's
 * player is one of these decks (`player.ts`, D30). Tempo, sync and every
 * mixer law live in Rust; this only forwards.
 */
import { useEffect, useRef, useState } from "react";
import { create } from "zustand";

import {
  audioDevices,
  type BeatMark,
  beatgridGet,
  type ChannelParam,
  DECKS,
  type FilterParam,
  type FilterUnit,
  type HotCueView,
  libraryTrack,
  type MasterParam,
  type MixConfig,
  type MixControl,
  type MixDeviceOptions,
  type MixState,
  mixControl,
  mixDeviceOptions,
  mixLoad,
  mixStart,
  mixStop,
  waveformColumns,
} from "../lib/ipc";
import { litCount } from "../lib/meterScale";
import {
  ASSIGN_VALUE,
  type Assign,
  cueFits,
  DEFAULT_TEMPO_RANGE,
  NUDGE,
  type TempoRange,
} from "../lib/mixView";

export type EngineStatus = "stopped" | "starting" | "running" | "error";

export interface DeckTrack {
  id: number;
  title: string | null;
  artist: string | null;
}

/** What the UI knows about a deck beyond the engine state. */
export interface DeckMeta {
  track: DeckTrack | null;
  /** Being decoded, if any. */
  loading: DeckTrack | null;
  error: string | null;
  hotCues: HotCueView[];
  /** Waveform columns (4 bytes each), null until fetched. */
  columns: Uint8Array | null;
  beats: BeatMark[] | null;
  /** The waveform or grid could not be fetched (the deck still plays). */
  viewError: string | null;
  tempoRange: TempoRange;
}

export interface ChannelValues {
  trim: number;
  eqHi: number;
  eqMid1: number;
  eqMid2: number;
  eqLo: number;
  fader: number;
  filterAssign: Assign;
  xfadeAssign: Assign;
  cue: boolean;
}

export interface FilterValues {
  freq: number;
  resonance: number;
  highPass: boolean;
  bandPass: boolean;
  lowPass: boolean;
  on: boolean;
  vcfToXfader: boolean;
  link: boolean;
}

export interface MasterValues {
  level: number;
  cueLevel: number;
  cueMix: number;
  splitCue: boolean;
  cuePostEq: boolean;
}

export interface MixStore {
  status: EngineStatus;
  error: string | null;
  /** The latest engine state while running. */
  live: MixState | null;
  config: MixConfig;
  devices: string[];
  options: MixDeviceOptions | null;
  deckCount: 2 | 4;
  decks: DeckMeta[];
  channels: ChannelValues[];
  filters: Record<FilterUnit, FilterValues>;
  crossfader: { pos: number; curve: number };
  master: MasterValues;
  /** The player's volume slider, 0..1, on top of the master level. */
  volume: number;

  loadDevices: () => Promise<void>;
  setConfig: (patch: Partial<MixConfig>) => Promise<void>;
  /**
   * Picks the output device (`null`: system default). A running output
   * reopens on it with the loaded tracks back where they were.
   */
  setDevice: (device: string | null) => Promise<void>;
  start: () => Promise<boolean>;
  stop: () => Promise<void>;
  setDeckCount: (n: 2 | 4) => void;
  /** Loads `track` on `deck`, starting the output if it is off; `play` plays it once loaded. */
  load: (deck: number, track: DeckTrack | number, play?: boolean) => Promise<void>;
  unload: (deck: number) => void;
  send: (c: MixControl) => void;
  togglePlay: (deck: number) => void;
  /** CUE pressed (`down`) or let go. */
  cue: (deck: number, down: boolean) => void;
  toggleSync: (deck: number, bar?: boolean) => void;
  /** -1 slower, 1 faster, 0 released. */
  nudge: (deck: number, dir: -1 | 0 | 1) => void;
  setTempo: (deck: number, ratio: number) => void;
  setTempoRange: (deck: number, range: TempoRange) => void;
  setChannel: <K extends keyof ChannelValues>(
    ch: number,
    param: K,
    value: ChannelValues[K],
  ) => void;
  setFilter: <K extends keyof FilterValues>(
    unit: FilterUnit,
    param: K,
    value: FilterValues[K],
  ) => void;
  setCrossfader: (patch: Partial<{ pos: number; curve: number }>) => void;
  setMaster: <K extends keyof MasterValues>(param: K, value: MasterValues[K]) => void;
  setVolume: (volume: number) => void;
}

export function emptyDeck(): DeckMeta {
  return {
    track: null,
    loading: null,
    error: null,
    hotCues: [],
    columns: null,
    beats: null,
    viewError: null,
    tempoRange: DEFAULT_TEMPO_RANGE,
  };
}

const DEFAULT_CHANNEL: ChannelValues = {
  trim: 0.5,
  eqHi: 0.5,
  eqMid1: 0.5,
  eqMid2: 0.5,
  eqLo: 0.5,
  fader: 1,
  filterAssign: "off",
  xfadeAssign: "off",
  cue: false,
};

const DEFAULT_FILTER: FilterValues = {
  freq: 0.5,
  resonance: 0,
  highPass: false,
  bandPass: false,
  lowPass: true,
  on: false,
  vcfToXfader: false,
  link: false,
};

export const DEFAULT_CONFIG: MixConfig = {
  device: null,
  sampleRate: null,
  bufferFrames: null,
  cue: { type: "off" },
};

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

const bit = (on: boolean) => (on ? 1 : 0);

function channelControl<K extends keyof ChannelValues>(
  ch: number,
  param: K,
  value: ChannelValues[K],
): MixControl {
  const v: number =
    typeof value === "boolean"
      ? bit(value)
      : typeof value === "string"
        ? ASSIGN_VALUE[value as Assign]
        : (value as number);
  return { type: "channel", ch, param: param as ChannelParam, value: v };
}

/** Every mixer position, so the engine matches the UI after a start. */
function mixerControls(s: MixStore): MixControl[] {
  const out: MixControl[] = [];
  s.channels.forEach((c, ch) => {
    for (const k of Object.keys(c) as (keyof ChannelValues)[]) {
      out.push(channelControl(ch, k, c[k]));
    }
  });
  for (const unit of ["x", "y"] as const) {
    const f = s.filters[unit];
    for (const k of Object.keys(f) as (keyof FilterValues)[]) {
      if (k === "link" && unit === "x") continue;
      const v = f[k];
      out.push({
        type: "filter",
        unit,
        param: k as FilterParam,
        value: typeof v === "boolean" ? bit(v) : v,
      });
    }
  }
  out.push({ type: "crossfader", ...s.crossfader });
  for (const k of Object.keys(s.master) as (keyof MasterValues)[]) {
    const v = s.master[k];
    out.push({
      type: "master",
      param: k as MasterParam,
      value: typeof v === "boolean" ? bit(v) : v,
    });
  }
  out.push({ type: "volume", value: s.volume });
  return out;
}

/** Where a loaded deck is, to put it back after the output reopens. */
interface DeckResume {
  deck: number;
  track: DeckTrack;
  positionMs: number;
  playing: boolean;
  tempo: number;
}

/** Bumped by each load per deck; a load that finishes after a newer one is dropped. */
const loads = Array.from({ length: DECKS }, () => 0);

export const useMix = create<MixStore>((set, get) => {
  function patchDeck(deck: number, patch: Partial<DeckMeta>) {
    set((s) => ({ decks: s.decks.map((d, i) => (i === deck ? { ...d, ...patch } : d)) }));
  }

  function onState(live: MixState) {
    if (get().status !== "running") return;
    if (live.deviceLost) {
      set({
        live,
        status: "error",
        error: `The audio device "${live.device}" went away. Start the output again.`,
      });
      return;
    }
    set({ live });
  }

  /** The user picked a cue routing; until then each device gets its own default. */
  let cueChosen = false;

  /**
   * New device options. Preselects the device's cue routing unless the user
   * chose one that the device can take.
   */
  function setOptions(options: MixDeviceOptions) {
    set((s) => {
      const keep = cueChosen && cueFits(s.config.cue, options.channels);
      return { options, config: keep ? s.config : { ...s.config, cue: options.defaultCue } };
    });
  }

  function deckView(deck: number) {
    return get().live?.decks[deck];
  }

  return {
    status: "stopped",
    error: null,
    live: null,
    config: DEFAULT_CONFIG,
    devices: [],
    options: null,
    deckCount: 2,
    decks: Array.from({ length: DECKS }, emptyDeck),
    // Channels 1–2 on filter X (left), 3–4 on Y (right), as in reks-mix.
    channels: Array.from({ length: DECKS }, (_, ch) => ({
      ...DEFAULT_CHANNEL,
      filterAssign: ch < 2 ? ("x" as const) : ("y" as const),
    })),
    filters: { x: { ...DEFAULT_FILTER }, y: { ...DEFAULT_FILTER, lowPass: false, highPass: true } },
    crossfader: { pos: 0.5, curve: 0 },
    master: { level: 0.8, cueLevel: 0.7, cueMix: 0, splitCue: false, cuePostEq: false },
    volume: 1,

    async loadDevices() {
      try {
        const [devices, options] = await Promise.all([
          audioDevices(),
          mixDeviceOptions(get().config.device),
        ]);
        set({ devices });
        setOptions(options);
      } catch (e) {
        set({ error: message(e) });
      }
    },

    async setConfig(patch) {
      const deviceChanged = "device" in patch && patch.device !== get().config.device;
      // A new device has its own rates and buffer sizes: back to its defaults.
      const reset = deviceChanged ? { sampleRate: null, bufferFrames: null } : {};
      if ("cue" in patch) cueChosen = true;
      set((s) => ({ config: { ...s.config, ...reset, ...patch } }));
      if (deviceChanged) {
        try {
          setOptions(await mixDeviceOptions(get().config.device));
        } catch (e) {
          set({ options: null, error: message(e) });
        }
      }
    },

    async setDevice(device) {
      const s = get();
      if (device === s.config.device) return;
      const resume: DeckResume[] = [];
      if (s.status === "running") {
        s.decks.forEach((meta, deck) => {
          const v = s.live?.decks[deck];
          if (meta.track === null || v?.loaded !== true) return;
          const { positionMs, playing, tempo } = v;
          resume.push({ deck, track: meta.track, positionMs, playing, tempo });
        });
      }
      await get().setConfig({ device });
      if (s.status !== "running") return;
      if (!(await get().start())) return;
      for (const r of resume) {
        await get().load(r.deck, r.track);
        if (get().decks[r.deck]?.track?.id !== r.track.id) continue;
        get().send({ type: "seek", deck: r.deck, ms: r.positionMs });
        if (r.tempo !== 1) get().send({ type: "tempo", deck: r.deck, ratio: r.tempo });
        if (r.playing) get().send({ type: "play", deck: r.deck });
      }
    },

    async start() {
      if (get().status === "starting") return false;
      set({ status: "starting", error: null, live: null });
      try {
        const live = await mixStart(get().config, onState);
        set({
          status: "running",
          live,
          decks: Array.from({ length: DECKS }, (_, i) => ({
            ...emptyDeck(),
            tempoRange: get().decks[i]?.tempoRange ?? DEFAULT_TEMPO_RANGE,
          })),
        });
        for (const c of mixerControls(get())) await mixControl(c);
        return true;
      } catch (e) {
        set({ status: "error", error: message(e), live: null });
        return false;
      }
    },

    async stop() {
      set({
        status: "stopped",
        live: null,
        error: null,
        decks: Array.from({ length: DECKS }, emptyDeck),
      });
      try {
        await mixStop();
      } catch (e) {
        set({ error: message(e) });
      }
    },

    setDeckCount: (n) => set({ deckCount: n }),

    async load(deck, track, play = false) {
      const id = typeof track === "number" ? track : track.id;
      // Playing the track that is already playing keeps it going.
      const v = deckView(deck);
      if (play && v?.playing === true && get().decks[deck]?.track?.id === id) return;
      const seq = (loads[deck] ?? 0) + 1;
      loads[deck] = seq;
      const current = () => loads[deck] === seq;
      let meta: DeckTrack =
        typeof track === "number" ? { id: track, title: null, artist: null } : track;
      patchDeck(deck, { loading: meta, error: null });
      try {
        if (get().status !== "running") {
          const started = await get().start();
          // Starting empties the decks; show this load again.
          if (current()) patchDeck(deck, { loading: started ? meta : null });
          if (!started) return;
        }
        if (typeof track === "number") {
          const t = (await libraryTrack(track)).track;
          meta = { id: track, title: t.title, artist: t.artist };
          if (current()) patchDeck(deck, { loading: meta });
        }
        const loaded = await mixLoad(deck, meta.id);
        if (loaded === null || !current()) return;
        patchDeck(deck, {
          track: meta,
          loading: null,
          hotCues: loaded.hotCues,
          columns: null,
          beats: null,
          viewError: null,
        });
        if (play) get().send({ type: "play", deck });
        const [columns, grid] = await Promise.allSettled([
          waveformColumns(meta.id),
          beatgridGet(meta.id),
        ]);
        if (!current()) return;
        const failed = [columns, grid].find((r) => r.status === "rejected");
        patchDeck(deck, {
          columns: columns.status === "fulfilled" ? new Uint8Array(columns.value) : null,
          beats: grid.status === "fulfilled" ? grid.value.beats : null,
          viewError: failed === undefined ? null : message(failed.reason),
        });
      } catch (e) {
        if (current()) patchDeck(deck, { loading: null, error: message(e) });
      }
    },

    unload(deck) {
      loads[deck] = (loads[deck] ?? 0) + 1;
      patchDeck(deck, {
        ...emptyDeck(),
        tempoRange: get().decks[deck]?.tempoRange ?? DEFAULT_TEMPO_RANGE,
      });
      get().send({ type: "unload", deck });
    },

    send(c) {
      if (get().status !== "running") return;
      mixControl(c).catch((e) => set({ error: message(e) }));
    },

    togglePlay(deck) {
      const v = deckView(deck);
      if (!v?.loaded) return;
      // PLAY during a cue preview keeps the deck playing (CDJ).
      get().send({ type: v.playing && !v.cuePreview ? "pause" : "play", deck });
    },

    cue(deck, down) {
      get().send({ type: down ? "cue" : "cueRelease", deck });
    },

    toggleSync(deck, bar) {
      const v = deckView(deck);
      if (v === undefined) return;
      const nextBar = bar ?? v.syncBar;
      // The BAR switch alone keeps sync as it is.
      const on = bar === undefined ? !v.sync : v.sync;
      get().send({ type: "sync", deck, on, bar: nextBar });
    },

    nudge(deck, dir) {
      get().send({ type: "nudge", deck, ratio: 1 + dir * NUDGE });
    },

    setTempo(deck, ratio) {
      get().send({ type: "tempo", deck, ratio });
    },

    setTempoRange(deck, range) {
      patchDeck(deck, { tempoRange: range });
    },

    setChannel(ch, param, value) {
      set((s) => ({
        channels: s.channels.map((c, i) => (i === ch ? { ...c, [param]: value } : c)),
      }));
      get().send(channelControl(ch, param, value));
    },

    setFilter(unit, param, value) {
      set((s) => ({ filters: { ...s.filters, [unit]: { ...s.filters[unit], [param]: value } } }));
      get().send({
        type: "filter",
        unit,
        param: param as FilterParam,
        value: typeof value === "boolean" ? bit(value) : (value as number),
      });
    },

    setCrossfader(patch) {
      const crossfader = { ...get().crossfader, ...patch };
      set({ crossfader });
      get().send({ type: "crossfader", ...crossfader });
    },

    setMaster(param, value) {
      set((s) => ({ master: { ...s.master, [param]: value } }));
      get().send({
        type: "master",
        param: param as MasterParam,
        value: typeof value === "boolean" ? bit(value) : (value as number),
      });
    },

    setVolume(volume) {
      set({ volume });
      get().send({ type: "volume", value: volume });
    },
  };
});

/** How long a clip stays lit; the engine reports it for one state only. */
export const CLIP_HOLD_MS = 1000;

/**
 * One meter from the live engine state: a channel (0..3) or the master.
 * Subscribes to that meter alone, so the rest of the mixer does not render
 * at the state rate.
 */
export function useMeter(source: number | "master"): { lit: number; clip: boolean } {
  const pick = (s: MixStore) =>
    source === "master" ? s.live?.masterMeter : s.live?.meters[source];
  // The lit LED count, not the level: the meter renders only when it changes.
  const lit = useMix((s) => litCount(pick(s)?.level ?? 0));
  const clip = useMix((s) => pick(s)?.clip ?? false);
  const [held, setHeld] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => {
    if (!clip) return;
    setHeld(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setHeld(false), CLIP_HOLD_MS);
  }, [clip]);
  useEffect(() => () => clearTimeout(timer.current), []);
  return { lit, clip: clip || held };
}
