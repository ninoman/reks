/**
 * Dev-only mixer backend for the mock (`mockBackend.ts`): four decks that
 * advance in real time and stream their state at 60 Hz, so the mixer UI runs
 * in a plain browser. Every track plays the 30 s fixture `mockGrid.ts` draws
 * (128 BPM, first beat at 12 ms; tracks without analysis have no grid). Sync
 * is a rough stand-in: it copies the leader's tempo and beat phase at once.
 * The real engine is `reks-mix` (tested there); mixer controls are accepted
 * and ignored, as in the engine until phase 5 tasks 6–7.
 */

import type { Channel } from "@tauri-apps/api/core";
import type { DeckLoaded, DeckView, MeterView, MixConfig, MixControl, MixState } from "../lib/ipc";

const DURATION_MS = 30_000;
const FIRST_BEAT_MS = 12;
const GRID_BPM = 128;
const BEAT_MS = 60_000 / GRID_BPM;
const FRAME_MS = 1000 / 60;

interface Deck {
  view: DeckView;
  grid: boolean;
  hotCues: number[];
  /** When playing started, for the automatic leader. */
  since: number;
}

function emptyDeck(): Deck {
  return {
    view: {
      trackId: null,
      loaded: false,
      playing: false,
      positionMs: 0,
      durationMs: 0,
      cueMs: 0,
      tempo: 1,
      nudge: 1,
      speed: 1,
      bpm: null,
      beat: null,
      cuePreview: false,
      sync: false,
      syncBar: false,
      locked: false,
      quantise: true,
    },
    grid: false,
    hotCues: [],
    since: 0,
  };
}

let decks: Deck[] = [];
let chosenLeader: number | null = null;
let timer: ReturnType<typeof setInterval> | null = null;
let config: MixConfig | null = null;

const beatAt = (ms: number) => (ms - FIRST_BEAT_MS) / BEAT_MS;
const msAtBeat = (beat: number) => FIRST_BEAT_MS + beat * BEAT_MS;

function leader(): number | null {
  if (chosenLeader !== null) return chosenLeader;
  let best: number | null = null;
  decks.forEach((d, i) => {
    if (!d.view.playing || !d.grid) return;
    if (best === null || d.since < (decks[best]?.since ?? 0)) best = i;
  });
  return best;
}

function tick() {
  const lead = leader();
  const leadDeck = lead === null ? undefined : decks[lead];
  const follows = (d: Deck, i: number) =>
    d.view.sync && leadDeck !== undefined && lead !== i && d.grid && leadDeck.grid;
  // Move every deck, then put followers in the leader's phase.
  decks.forEach((d, i) => {
    const v = d.view;
    if (!v.loaded) return;
    v.speed = follows(d, i) && leadDeck ? leadDeck.view.speed : v.tempo * v.nudge;
    if (v.playing) {
      v.positionMs += FRAME_MS * v.speed;
      if (v.positionMs >= v.durationMs) {
        v.positionMs = v.durationMs;
        v.playing = false;
      }
    }
  });
  decks.forEach((d, i) => {
    const v = d.view;
    if (!v.loaded) return;
    v.locked = follows(d, i);
    if (v.locked && leadDeck !== undefined && v.playing && v.nudge === 1) {
      // Jump into the leader's beat phase (bar phase with bar sync).
      const span = v.syncBar ? 4 : 1;
      const own = beatAt(v.positionMs);
      const diff = beatAt(leadDeck.view.positionMs) - own;
      const wrapped = diff - span * Math.round(diff / span);
      v.positionMs = msAtBeat(own + wrapped);
    }
    v.bpm = d.grid ? GRID_BPM * v.speed : null;
    v.beat = d.grid ? beatAt(v.positionMs) : null;
  });
}

function snapshot(): MixState {
  return {
    device: config?.device ?? "MacBook Pro Speakers",
    sampleRate: config?.sampleRate ?? 44_100,
    channels: config?.cue.type === "outputs" ? 4 : 2,
    bufferFrames: config?.bufferFrames ?? null,
    deviceLost: false,
    xruns: 0,
    leader: leader(),
    leaderChosen: chosenLeader !== null,
    decks: decks.map((d) => ({ ...d.view })),
    meters: decks.map((d) => meter(d.view.playing)),
    masterMeter: meter(decks.some((d) => d.view.playing)),
  };
}

/** A plausible moving level while playing. */
function meter(playing: boolean): MeterView {
  return { level: playing ? 0.45 + 0.25 * Math.abs(Math.sin(Date.now() / 90)) : 0, clip: false };
}

function snap(d: Deck, ms: number): number {
  if (!d.view.quantise || !d.grid) return ms;
  return msAtBeat(Math.round(beatAt(ms)));
}

function control(c: MixControl) {
  if (!("deck" in c) || c.type === "leader") {
    if (c.type === "leader") chosenLeader = c.deck;
    return;
  }
  const d = decks[c.deck];
  if (d === undefined) throw new Error(`no deck ${c.deck + 1}`);
  const v = d.view;
  switch (c.type) {
    case "unload":
      decks[c.deck] = emptyDeck();
      break;
    case "play":
      v.cuePreview = false;
      if (v.loaded && !v.playing) {
        v.playing = true;
        d.since = performance.now();
      }
      break;
    case "pause":
      v.playing = false;
      v.cuePreview = false;
      break;
    case "cue":
      if (v.playing) {
        v.playing = false;
        v.cuePreview = false;
        v.positionMs = v.cueMs;
      } else if (Math.abs(v.positionMs - v.cueMs) < 1 && v.loaded) {
        // Held on the cue point: preview until let go.
        v.playing = true;
        v.cuePreview = true;
        d.since = performance.now();
      } else {
        v.cueMs = snap(d, v.positionMs);
        v.positionMs = v.cueMs;
      }
      break;
    case "cueRelease":
      if (v.cuePreview) {
        v.cuePreview = false;
        v.playing = false;
        v.positionMs = v.cueMs;
      }
      break;
    case "seek":
      v.positionMs = Math.min(v.durationMs, Math.max(0, v.playing ? c.ms : snap(d, c.ms)));
      break;
    case "hotCue": {
      const at = d.hotCues[c.slot];
      if (at !== undefined) {
        v.cuePreview = false;
        v.positionMs = at;
        v.playing = true;
        d.since = performance.now();
      }
      break;
    }
    case "tempo":
      v.tempo = Math.min(2, Math.max(0, c.ratio));
      break;
    case "nudge":
      v.nudge = c.ratio;
      break;
    case "sync":
      v.sync = c.on;
      v.syncBar = c.bar;
      break;
    case "quantise":
      v.quantise = c.on;
      break;
  }
}

function load(deck: number, id: number, analysed: boolean): DeckLoaded {
  if (config === null) throw new Error("the mixer's audio output is not started");
  const d = emptyDeck();
  const prev = decks[deck];
  d.view = {
    ...d.view,
    trackId: id,
    loaded: true,
    durationMs: DURATION_MS,
    tempo: prev?.view.tempo ?? 1,
    sync: prev?.view.sync ?? false,
    syncBar: prev?.view.syncBar ?? false,
    quantise: prev?.view.quantise ?? true,
  };
  d.grid = analysed;
  // Hot cues A, B and D on bars 2, 5 and 9 (D without a colour); a memory cue on bar 7.
  d.hotCues = analysed ? [msAtBeat(4), msAtBeat(16), msAtBeat(32)] : [];
  const slots = [0, 1, 3];
  const colours = [0x30d158, 0xff375f, null];
  decks[deck] = d;
  return {
    deck,
    trackId: id,
    durationMs: DURATION_MS,
    hotCues: d.hotCues.map((positionMs, i) => ({
      slot: slots[i] ?? i,
      positionMs,
      colorRgb: colours[i] ?? null,
    })),
    memoryCues: analysed ? [{ positionMs: msAtBeat(24), colorRgb: null }] : [],
  };
}

export function mockMix(
  cmd: string,
  args: Record<string, unknown>,
  analysed: (id: number) => boolean,
): unknown {
  switch (cmd) {
    case "mix_device_options":
      return args.device === "USB Audio Interface"
        ? {
            rates: [44_100, 48_000, 88_200, 96_000],
            defaultRate: 48_000,
            channels: 4,
            defaultCue: { type: "outputs", master: [0, 1], cue: [2, 3] },
            bufferFrames: [32, 2048],
          }
        : {
            rates: [44_100, 48_000],
            defaultRate: 48_000,
            channels: 2,
            defaultCue: { type: "off" },
            bufferFrames: [64, 4096],
          };
    case "mix_start": {
      config = args.config as MixConfig;
      decks = Array.from({ length: 4 }, emptyDeck);
      chosenLeader = null;
      const channel = args.onState as Channel<MixState>;
      if (timer !== null) clearInterval(timer);
      timer = setInterval(() => {
        tick();
        channel.onmessage(snapshot());
      }, FRAME_MS);
      // Opening a device takes a moment.
      return new Promise((resolve) => setTimeout(() => resolve(snapshot()), 400));
    }
    case "mix_stop":
      if (timer !== null) clearInterval(timer);
      timer = null;
      config = null;
      decks = [];
      return null;
    case "mix_load": {
      const deck = args.deck as number;
      const id = args.id as number;
      const loaded = load(deck, id, analysed(id));
      // Decoding takes a moment.
      return new Promise((resolve) => setTimeout(() => resolve(loaded), 300));
    }
    case "mix_control":
      if (config === null) throw new Error("the mixer's audio output is not started");
      control(args.control as MixControl);
      return null;
    default:
      return undefined;
  }
}
