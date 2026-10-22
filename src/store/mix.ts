/**
 * Mixer (Zustand): starts and stops the `reks-mix` engine, mirrors the state
 * it streams (about 60 a second), loads library tracks onto decks with their
 * waveform and beat lines, and keeps the mixer's knob and switch positions
 * and the player's volume, which it sends as `MixControl`s. The library's
 * player is one of these decks (`player.ts`, D30). A track loaded without
 * a grid is queued for analysis and picks the grid up when it is done. Each
 * deck's pad mode, pages and last jump / loop sizes live here. Tempo, sync
 * and every mixer law live in Rust; this only forwards.
 */
import { useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { DEFAULT_DECK_ZOOM, stepZoom } from "../lib/deckScroll";
import {
  analysisReanalyse,
  audioDevices,
  type BeatMark,
  beatgridGet,
  type ChannelParam,
  DECKS,
  type FilterParam,
  type FilterUnit,
  type HotCueView,
  type Key,
  libraryTrack,
  type MasterParam,
  type MemoryCueView,
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
  DEFAULT_PAD_BEATS,
  DEFAULT_TEMPO_RANGE,
  loopPadControls,
  NUDGE,
  nextPadMode,
  type PadMode,
  type TempoRange,
  tempoForBpm,
} from "../lib/mixView";

export type EngineStatus = "stopped" | "starting" | "running" | "error";

/** What a load is asked for: a library track, with what the caller already shows of it. */
export interface TrackRef {
  id: number;
  title: string | null;
  artist: string | null;
}

export interface DeckTrack extends TrackRef {
  key: Key | null;
  artworkId: number | null;
  artworkMime: string;
}

/** Pad modes with pages of sizes. */
export type PagedPadMode = Exclude<PadMode, "hotCue">;

/** What the UI knows about a deck beyond the engine state. */
export interface DeckMeta {
  track: DeckTrack | null;
  /** Being decoded, if any. */
  loading: TrackRef | null;
  error: string | null;
  /** The track whose load failed, for Retry. */
  failed: TrackRef | null;
  hotCues: HotCueView[];
  memoryCues: MemoryCueView[];
  /** Waveform columns (4 bytes each), null until fetched. */
  columns: Uint8Array | null;
  beats: BeatMark[] | null;
  /** The waveform or grid could not be fetched (the deck still plays). */
  viewError: string | null;
  /** Loaded without a grid and queued for analysis. */
  analysing: boolean;
  /** Analysis gave the track a grid the engine has not got yet: it reloads when the deck is paused. */
  gridPending: boolean;
  // Kept across loads:
  tempoRange: TempoRange;
  /** Scrolling waveform zoom (`DECK_ZOOM_S` index). */
  zoom: number;
  padMode: PadMode;
  /** Page shown per paged mode (`JUMP_PAGES` / `LOOP_PAGES` index). */
  padPage: Record<PagedPadMode, number>;
  /** Beats of the jump and loop used last; the keys use them. */
  lastJump: number;
  lastLoop: number;
}

/** The deck settings a load or eject keeps. */
type DeckPrefs = Pick<
  DeckMeta,
  "tempoRange" | "zoom" | "padMode" | "padPage" | "lastJump" | "lastLoop"
>;

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
  load: (deck: number, track: TrackRef | number, play?: boolean) => Promise<void>;
  unload: (deck: number) => void;
  /** Loads the track that failed again, or fetches a failed waveform again. */
  retry: (deck: number) => Promise<void>;
  /** A track's analysis finished (`ok`) or failed: decks waiting on it pick up the grid. */
  analysisDone: (trackId: number, ok: boolean) => void;
  send: (c: MixControl) => void;
  togglePlay: (deck: number) => void;
  /** CUE pressed (`down`) or let go. */
  cue: (deck: number, down: boolean) => void;
  /** SYNC on or off; `bar` lines up the downbeats too (Settings → Decks). */
  toggleSync: (deck: number, bar: boolean) => void;
  takeMaster: (deck: number) => void;
  /** Beat jump by `beats` (negative: back); remembers the size for the keys. */
  beatJump: (deck: number, beats: number) => void;
  /** Beat jump by the size used last, back (-1) or forward. */
  jumpLast: (deck: number, dir: -1 | 1) => void;
  /** A BEAT LOOP pad: set a loop of `beats`, resize the active one to it, or leave it. */
  loopPad: (deck: number, beats: number) => void;
  /** Loop on at the size used last, or off. */
  toggleLoop: (deck: number) => void;
  setPadMode: (deck: number, mode: PadMode) => void;
  cyclePadMode: (deck: number) => void;
  setPadPage: (deck: number, mode: PagedPadMode, page: number) => void;
  /** Sets the tempo that plays the deck at `bpm`, stepping the range up when needed. */
  setBpm: (deck: number, bpm: number) => void;
  /** -1 slower, 1 faster, 0 released. */
  nudge: (deck: number, dir: -1 | 0 | 1) => void;
  setTempo: (deck: number, ratio: number) => void;
  setTempoRange: (deck: number, range: TempoRange) => void;
  /** Zooms the deck's scrolling waveform in (1) or out (-1). */
  zoomDeck: (deck: number, dir: -1 | 1) => void;
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

const DEFAULT_PREFS: DeckPrefs = {
  tempoRange: DEFAULT_TEMPO_RANGE,
  zoom: DEFAULT_DECK_ZOOM,
  padMode: "hotCue",
  padPage: { beatJump: 0, beatLoop: 0 },
  lastJump: DEFAULT_PAD_BEATS,
  lastLoop: DEFAULT_PAD_BEATS,
};

export function emptyDeck(): DeckMeta {
  return {
    track: null,
    loading: null,
    error: null,
    failed: null,
    hotCues: [],
    memoryCues: [],
    columns: null,
    beats: null,
    viewError: null,
    analysing: false,
    gridPending: false,
    ...DEFAULT_PREFS,
  };
}

/** An empty deck that keeps `prev`'s settings. */
function emptyDeckFrom(prev: DeckMeta | undefined): DeckMeta {
  if (prev === undefined) return emptyDeck();
  const { tempoRange, zoom, padMode, padPage, lastJump, lastLoop } = prev;
  return { ...emptyDeck(), tempoRange, zoom, padMode, padPage, lastJump, lastLoop };
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
    get().decks.forEach((meta, deck) => {
      if (meta.gridPending && live.decks[deck]?.playing === false) void reloadForGrid(deck);
    });
  }

  /**
   * Reloads a paused deck's track so the engine gets the grid analysis
   * just stored, putting the cue point, position and tempo back.
   */
  async function reloadForGrid(deck: number) {
    const meta = get().decks[deck];
    const v = deckView(deck);
    if (meta?.track == null || v?.loaded !== true || v.playing) return;
    patchDeck(deck, { gridPending: false });
    const seq = (loads[deck] ?? 0) + 1;
    loads[deck] = seq;
    const { positionMs, cueMs, tempo } = v;
    try {
      const loaded = await mixLoad(deck, meta.track.id);
      if (loaded === null || loads[deck] !== seq) return;
      patchDeck(deck, { hotCues: loaded.hotCues, memoryCues: loaded.memoryCues });
      if (cueMs > 0) {
        // CUE on a paused deck off its cue point sets it at the head.
        get().send({ type: "seek", deck, ms: cueMs });
        get().send({ type: "cue", deck });
      }
      get().send({ type: "seek", deck, ms: positionMs });
      get().send({ type: "tempo", deck, ratio: tempo });
    } catch (e) {
      if (loads[deck] === seq) patchDeck(deck, { viewError: message(e) });
    }
  }

  /** The waveform and grid of the track on `deck`, for the views. */
  async function fetchViews(deck: number, id: number, current: () => boolean) {
    const [columns, grid] = await Promise.allSettled([waveformColumns(id), beatgridGet(id)]);
    if (!current()) return null;
    const failed = [columns, grid].find((r) => r.status === "rejected");
    const beats = grid.status === "fulfilled" ? grid.value.beats : null;
    patchDeck(deck, {
      columns: columns.status === "fulfilled" ? new Uint8Array(columns.value) : null,
      beats,
      viewError: failed === undefined ? null : message(failed.reason),
    });
    return beats;
  }

  function prefs(deck: number): DeckMeta {
    return get().decks[deck] ?? emptyDeck();
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
          decks: Array.from({ length: DECKS }, (_, i) => emptyDeckFrom(get().decks[i])),
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
        decks: Array.from({ length: DECKS }, (_, i) => emptyDeckFrom(get().decks[i])),
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
      const ref: TrackRef =
        typeof track === "number"
          ? { id: track, title: null, artist: null }
          : { id: track.id, title: track.title, artist: track.artist };
      let meta: DeckTrack = { ...ref, key: null, artworkId: null, artworkMime: "" };
      patchDeck(deck, { loading: ref, error: null, failed: null });
      try {
        if (get().status !== "running") {
          const started = await get().start();
          // Starting empties the decks; show this load again.
          if (current()) patchDeck(deck, { loading: started ? meta : null });
          if (!started) return;
        }
        // Key and artwork for the header; without them the deck still loads.
        let t: Awaited<ReturnType<typeof libraryTrack>>["track"] | null = null;
        try {
          t = (await libraryTrack(ref.id)).track;
        } catch {
          // Removed meanwhile, or the library is busy: load with what we have.
        }
        if (t !== null) {
          meta = {
            id: ref.id,
            title: t.title,
            artist: t.artist,
            key: t.key,
            artworkId: t.artwork?.id ?? null,
            artworkMime: t.artwork?.mime ?? "",
          };
        }
        if (current()) patchDeck(deck, { loading: ref.title === null ? meta : ref });
        const loaded = await mixLoad(deck, meta.id);
        if (loaded === null || !current()) return;
        patchDeck(deck, {
          track: meta,
          loading: null,
          hotCues: loaded.hotCues,
          memoryCues: loaded.memoryCues,
          columns: null,
          beats: null,
          viewError: null,
          analysing: false,
          gridPending: false,
        });
        if (play) get().send({ type: "play", deck });
        const beats = await fetchViews(deck, meta.id, current);
        if (beats === null || beats.length > 0 || !current()) return;
        // No grid yet: analyse it first; the deck plays meanwhile.
        patchDeck(deck, { analysing: true });
        try {
          await analysisReanalyse([meta.id]);
        } catch {
          // Analysis is off (no beat model): the deck says it has no grid.
          if (current()) patchDeck(deck, { analysing: false });
        }
      } catch (e) {
        if (current()) patchDeck(deck, { loading: null, error: message(e), failed: ref });
      }
    },

    unload(deck) {
      loads[deck] = (loads[deck] ?? 0) + 1;
      patchDeck(deck, emptyDeckFrom(get().decks[deck]));
      get().send({ type: "unload", deck });
    },

    async retry(deck) {
      const meta = get().decks[deck];
      if (meta === undefined) return;
      if (meta.failed !== null) return get().load(deck, meta.failed);
      if (meta.track === null) return;
      const seq = loads[deck];
      await fetchViews(deck, meta.track.id, () => loads[deck] === seq);
    },

    analysisDone(trackId, ok) {
      get().decks.forEach((meta, deck) => {
        if (!meta.analysing || meta.track?.id !== trackId) return;
        const seq = loads[deck];
        const current = () => loads[deck] === seq;
        patchDeck(deck, { analysing: false });
        if (!ok) return;
        void fetchViews(deck, trackId, current).then((beats) => {
          if (beats !== null && beats.length > 0 && current()) {
            patchDeck(deck, { gridPending: true });
            if (deckView(deck)?.playing === false) void reloadForGrid(deck);
          }
        });
      });
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
      get().send({ type: "sync", deck, on: !v.sync, bar });
    },

    takeMaster(deck) {
      get().send({ type: "takeMaster", deck });
    },

    beatJump(deck, beats) {
      patchDeck(deck, { lastJump: Math.abs(beats) });
      get().send({ type: "beatJump", deck, beats });
    },

    jumpLast(deck, dir) {
      get().send({ type: "beatJump", deck, beats: dir * prefs(deck).lastJump });
    },

    loopPad(deck, beats) {
      patchDeck(deck, { lastLoop: beats });
      for (const c of loopPadControls(deckView(deck)?.loopRegion?.beats ?? null, beats)) {
        get().send({ ...c, deck });
      }
    },

    toggleLoop(deck) {
      const looping = (deckView(deck)?.loopRegion ?? null) !== null;
      get().send(
        looping
          ? { type: "loopExit", deck }
          : { type: "beatLoop", deck, beats: prefs(deck).lastLoop },
      );
    },

    setPadMode(deck, mode) {
      patchDeck(deck, { padMode: mode });
    },

    cyclePadMode(deck) {
      patchDeck(deck, { padMode: nextPadMode(prefs(deck).padMode) });
    },

    setPadPage(deck, mode, page) {
      patchDeck(deck, { padPage: { ...prefs(deck).padPage, [mode]: page } });
    },

    setBpm(deck, bpm) {
      const v = deckView(deck);
      if (v?.bpm == null || v.speed <= 0 || v.sync) return;
      // The track's own BPM here: what plays at tempo 1 (nudge left out).
      const t = tempoForBpm(bpm, v.bpm / v.speed, prefs(deck).tempoRange);
      if (t === null) return;
      if (t.range !== prefs(deck).tempoRange) patchDeck(deck, { tempoRange: t.range });
      get().send({ type: "tempo", deck, ratio: t.ratio });
    },

    nudge(deck, dir) {
      get().send({ type: "nudge", deck, ratio: 1 + dir * NUDGE });
    },

    setTempo(deck, ratio) {
      get().send({ type: "tempo", deck, ratio });
    },

    zoomDeck(deck, dir) {
      patchDeck(deck, { zoom: stepZoom(get().decks[deck]?.zoom ?? DEFAULT_DECK_ZOOM, dir) });
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
