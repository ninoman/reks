/**
 * Dev-only mixer backend for the mock (`mockBackend.ts`): four decks that
 * advance in real time and stream their state at 60 Hz, so the mixer UI runs
 * in a plain browser. Every track plays the 30 s fixture `mockGrid.ts` draws
 * (128 BPM, first beat at 12 ms; tracks without analysis have no grid). Sync
 * is a rough stand-in: it copies the master's tempo and beat phase at once.
 * The master is handed on as in the engine (a playing synced deck takes it
 * at its tempo when the master stops); beat jumps and loops use the grid.
 * Cues live per track for the session, as the library keeps them: edits
 * (`mix_cue_edit`) set, rename and delete them, MEM during a loop saves a
 * memory loop, and a playing deck enters an active (AUTO) memory loop at
 * its start.
 * The real engine is `reks-mix` (tested there); mixer controls are accepted
 * and ignored, as in the engine until phase 5 tasks 6–7.
 */

import type { Channel } from "@tauri-apps/api/core";
import type {
  CueEdit,
  DeckCues,
  DeckLoaded,
  DeckView,
  LoopView,
  MemoryCueView,
  MeterView,
  MixConfig,
  MixControl,
  MixState,
} from "../lib/ipc";

const DURATION_MS = 30_000;
const FIRST_BEAT_MS = 12;
const GRID_BPM = 128;
const BEAT_MS = 60_000 / GRID_BPM;
const FRAME_MS = 1000 / 60;

interface Deck {
  view: DeckView;
  grid: boolean;
  /** When playing started, for electing a master. */
  since: number;
}

/** As `reks-core` `HOT_CUE_PALETTE`. */
const PALETTE = [0x28e214, 0x305aff, 0xe62828, 0xc3af04, 0xb432ff, 0x00e0ff, 0xe0641b, 0xff127b];

/** Each track's cues, as the library would keep them. */
const trackCues = new Map<number, DeckCues>();
let nextCueId = 1;

/**
 * A track's cues; an analysed track starts with A, B and D on bars 2, 5
 * and 9, a memory cue on bar 7 and an auto memory loop over bar 11.
 */
function cuesOf(id: number, analysed: boolean): DeckCues {
  let c = trackCues.get(id);
  if (c === undefined) {
    const hot = (slot: number, beat: number, colorRgb: number | null, name: string | null) => ({
      id: nextCueId++,
      slot,
      positionMs: msAtBeat(beat),
      colorRgb,
      name,
    });
    c = analysed
      ? {
          hotCues: [
            hot(0, 4, 0x30d158, "Intro"),
            hot(1, 16, 0xff375f, "Drop"),
            hot(3, 32, null, null),
          ],
          memoryCues: [
            memory(msAtBeat(24), null, false),
            memory(msAtBeat(40), msAtBeat(44), true, "Break"),
          ],
        }
      : { hotCues: [], memoryCues: [] };
    trackCues.set(id, c);
  }
  return c;
}

function memory(
  positionMs: number,
  endMs: number | null,
  activeLoop: boolean,
  name: string | null = null,
): MemoryCueView {
  return { id: nextCueId++, positionMs, endMs, activeLoop, colorRgb: null, name };
}

function editCues(id: number, edit: CueEdit, atMs: number, loop: LoopView | null): DeckCues {
  const c = trackCues.get(id) ?? { hotCues: [], memoryCues: [] };
  let { hotCues, memoryCues } = c;
  switch (edit.type) {
    case "setHotCue": {
      if (hotCues.some((h) => h.slot === edit.slot)) throw new Error("hot cue already set");
      const used = hotCues.map((h) => h.colorRgb);
      const colorRgb = PALETTE.find((p) => !used.includes(p)) ?? PALETTE[edit.slot % 8] ?? null;
      hotCues = [
        ...hotCues,
        { id: nextCueId++, slot: edit.slot, positionMs: atMs, colorRgb, name: null },
      ];
      hotCues.sort((a, b) => a.slot - b.slot);
      break;
    }
    case "addMemoryCue": {
      const start = loop?.startMs ?? atMs;
      const end = loop?.endMs ?? null;
      const same = (m: MemoryCueView) =>
        Math.abs(m.positionMs - start) < 1 && Math.abs((m.endMs ?? 0) - (end ?? 0)) < 1;
      if (!memoryCues.some(same)) {
        memoryCues = [...memoryCues, memory(start, end, false)];
        memoryCues.sort((a, b) => a.positionMs - b.positionMs);
      }
      break;
    }
    case "setActiveLoop":
      memoryCues = memoryCues.map((m) =>
        m.id === edit.cueId && m.endMs !== null ? { ...m, activeLoop: edit.on } : m,
      );
      break;
    case "rename": {
      const name = edit.name?.trim() || null;
      hotCues = hotCues.map((h) => (h.id === edit.cueId ? { ...h, name } : h));
      memoryCues = memoryCues.map((m) => (m.id === edit.cueId ? { ...m, name } : m));
      break;
    }
    case "delete":
      hotCues = hotCues.filter((h) => h.id !== edit.cueId);
      memoryCues = memoryCues.filter((m) => m.id !== edit.cueId);
      break;
  }
  const out = { hotCues, memoryCues };
  trackCues.set(id, out);
  return out;
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
      masterTempo: false,
      master: false,
      loopRegion: null,
      loopIn: null,
      lastLoop: null,
    },
    grid: false,
    since: 0,
  };
}

/** ipc `PREVIEW_DECK`: the last slot, after the four mixer decks. */
const PREVIEW_DECK = 4;
let decks: Deck[] = [];
let master: number | null = null;
/** The master was playing at the last tick, so stopping hands it on. */
let masterPlaying = false;
const FINE_JUMP_MS = 10;
/** Shortest manual loop: two 4 ms loop crossfades (`reks-mix`). */
const MIN_LOOP_MS = 8;
let timer: ReturnType<typeof setInterval> | null = null;
let config: MixConfig | null = null;

const beatAt = (ms: number) => (ms - FIRST_BEAT_MS) / BEAT_MS;
const msAtBeat = (beat: number) => FIRST_BEAT_MS + beat * BEAT_MS;

/** A playing deck with a grid to take master: synced first, then the one playing longest. */
function elect(except: number | null): number | null {
  let best: number | null = null;
  let bestRank = Number.POSITIVE_INFINITY;
  decks.forEach((d, i) => {
    if (i === except || !d.view.playing || !d.grid) return;
    // Synced decks first, then by play start (`since` is under 1e12 ms).
    const rank = (d.view.locked ? 0 : 1e12) + d.since;
    if (rank < bestRank) {
      best = i;
      bestRank = rank;
    }
  });
  return best;
}

/** As in `reks-mix` `Graph::update_master`. */
function updateMaster() {
  const m = master === null ? undefined : decks[master];
  if (master !== null && (!m?.view.loaded || !m.grid || (masterPlaying && !m.view.playing))) {
    const next = elect(master);
    decks.forEach((d, i) => {
      if (i === master || (next !== null && next !== i) || !d.view.locked) return;
      // Keep the matched tempo, so nothing changes speed.
      d.view.tempo = d.view.speed / d.view.nudge;
    });
    master = next;
  }
  if (master === null) master = elect(null);
  masterPlaying = master !== null && decks[master]?.view.playing === true;
}

function tick() {
  updateMaster();
  const lead = master;
  const leadDeck = lead === null ? undefined : decks[lead];
  const follows = (d: Deck, i: number) =>
    d.view.sync && leadDeck !== undefined && lead !== i && d.grid && leadDeck.grid;
  // Move every deck, then put followers in the leader's phase.
  decks.forEach((d, i) => {
    const v = d.view;
    if (!v.loaded) return;
    v.speed = follows(d, i) && leadDeck ? leadDeck.view.speed : v.tempo * v.nudge;
    if (v.playing) {
      const before = v.positionMs;
      v.positionMs += FRAME_MS * v.speed;
      enterAutoLoop(d, before);
      const l = v.loopRegion;
      if (l && v.positionMs >= l.endMs) {
        v.positionMs = l.startMs + ((v.positionMs - l.startMs) % (l.endMs - l.startMs));
      }
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
    v.master = i === lead;
    if (
      v.locked &&
      leadDeck !== undefined &&
      givesPhase(leadDeck.view) &&
      v.playing &&
      v.nudge === 1 &&
      !v.loopRegion
    ) {
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

/** As `reks-mix` `sync::Lead`: a leader looping under a beat or part of a beat gives no phase. */
function givesPhase(v: DeckView): boolean {
  const n = v.loopRegion?.beats;
  if (v.loopRegion === null) return true;
  return n != null && n >= 1 - 1 / 64 && Math.abs(n - Math.round(n)) <= 1 / 64;
}

/** A loop measured in beats when the deck has a grid. */
function makeLoop(d: Deck, startMs: number, endMs: number): LoopView {
  return { startMs, endMs, beats: d.grid ? (endMs - startMs) / BEAT_MS : null };
}

/** As `reks-mix` `Deck::enter_auto_loop`: an active memory loop whose start the head passed. */
function enterAutoLoop(d: Deck, before: number) {
  const v = d.view;
  if (v.loopRegion !== null || v.trackId === null) return;
  const hit = trackCues
    .get(v.trackId)
    ?.memoryCues.find(
      (m) =>
        m.activeLoop && m.endMs !== null && m.positionMs >= before && m.positionMs < v.positionMs,
    );
  if (hit?.endMs == null) return;
  v.loopRegion = makeLoop(d, hit.positionMs, hit.endMs);
}

function snapshot(): MixState {
  return {
    device: config?.device ?? "MacBook Pro Speakers",
    sampleRate: config?.sampleRate ?? 44_100,
    channels: config?.cue.type === "outputs" ? 4 : 2,
    bufferFrames: config?.bufferFrames ?? null,
    deviceLost: false,
    xruns: 0,
    masterDeck: master,
    decks: decks.slice(0, PREVIEW_DECK).map((d) => ({ ...d.view })),
    preview: { ...(decks[PREVIEW_DECK] ?? emptyDeck()).view },
    meters: decks.slice(0, PREVIEW_DECK).map((d) => meter(d.view.playing)),
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

/** Leaves the active loop, keeping it for RELOOP. */
function exitLoop(v: DeckView) {
  if (v.loopRegion) v.lastLoop = v.loopRegion;
  v.loopRegion = null;
}

/** A jump out of the active loop ends it. */
function leaveLoopUnless(v: DeckView, ms: number) {
  const l = v.loopRegion;
  if (l && !(ms >= l.startMs && ms < l.endMs)) exitLoop(v);
}

/** The head for IN / OUT: the nearest beat with quantise. */
function loopPoint(d: Deck): number {
  return Math.min(d.view.durationMs, Math.max(0, snap(d, d.view.positionMs)));
}

function loopIn(d: Deck) {
  const v = d.view;
  const at = loopPoint(d);
  const l = v.loopRegion;
  if (!l) v.loopIn = at;
  else if (l.endMs - at >= MIN_LOOP_MS) v.loopRegion = makeLoop(d, at, l.endMs);
}

function loopOut(d: Deck) {
  const v = d.view;
  const at = loopPoint(d);
  const l = v.loopRegion;
  if (l) {
    if (at - l.startMs >= MIN_LOOP_MS) v.loopRegion = makeLoop(d, l.startMs, at);
    return;
  }
  const start = v.loopIn;
  if (start === null || start >= v.positionMs) return;
  const end = at > start ? at : d.grid ? Math.min(v.durationMs, start + BEAT_MS) : null;
  if (end === null || end - start < MIN_LOOP_MS) return;
  v.loopIn = null;
  v.loopRegion = makeLoop(d, start, end);
  if (!v.playing) v.positionMs = start;
}

function reloop(d: Deck) {
  const v = d.view;
  if (v.loopRegion) return exitLoop(v);
  const l = v.lastLoop;
  if (!l) return;
  let to = l.startMs;
  if (v.playing && v.quantise && d.grid) {
    // Keep the beat phase, landing a beat on when that is before the start.
    const shift = beatAt(v.positionMs) - beatAt(l.startMs);
    to = l.startMs + (shift - Math.floor(shift)) * BEAT_MS;
  }
  v.loopRegion = l;
  v.positionMs = to < l.endMs ? to : l.startMs;
}

function beatJump(d: Deck, beats: number) {
  const v = d.view;
  const to = v.positionMs + beats * BEAT_MS;
  if (!d.grid || to < 0 || to >= v.durationMs) return;
  const l = v.loopRegion;
  if (l) {
    const by = beats * BEAT_MS;
    if (l.startMs + by < 0 || l.endMs + by > v.durationMs) return;
    v.loopRegion = { ...l, startMs: l.startMs + by, endMs: l.endMs + by };
  }
  v.positionMs = to;
}

function beatLoop(d: Deck, beats: number) {
  const v = d.view;
  if (!d.grid) return;
  const q = Math.min(beats, 1);
  const startMs = v.quantise
    ? msAtBeat(Math.floor(beatAt(v.positionMs) / q + 1e-9) * q)
    : v.positionMs;
  const endMs = startMs + beats * BEAT_MS;
  if (startMs >= 0 && endMs <= v.durationMs) v.loopRegion = { startMs, endMs, beats };
}

/** Halve or double from the start: along the grid, or by time without one. */
function resizeLoop(v: DeckView, double: boolean) {
  const l = v.loopRegion;
  if (!l) return;
  const factor = double ? 2 : 0.5;
  const endMs = l.startMs + (l.endMs - l.startMs) * factor;
  if (endMs > v.durationMs) return;
  if (l.beats === null) {
    if (endMs - l.startMs >= MIN_LOOP_MS) v.loopRegion = { ...l, endMs };
    return;
  }
  const beats = l.beats * factor;
  if (beats >= 1 / 32 && beats <= 512) v.loopRegion = { ...l, endMs, beats };
}

function control(c: MixControl) {
  if (!("deck" in c)) return;
  const d = decks[c.deck];
  if (d === undefined) throw new Error(`no deck ${c.deck + 1}`);
  const v = d.view;
  switch (c.type) {
    case "takeMaster":
      if (d.grid) {
        if (v.locked) v.tempo = v.speed / v.nudge;
        master = c.deck;
        masterPlaying = v.playing;
      }
      break;
    case "beatJump":
      beatJump(d, c.beats);
      break;
    case "fineJump":
      v.positionMs = Math.min(
        v.durationMs,
        Math.max(0, v.positionMs + (c.forward ? FINE_JUMP_MS : -FINE_JUMP_MS)),
      );
      break;
    case "beatLoop":
      beatLoop(d, c.beats);
      break;
    case "loopExit":
      exitLoop(v);
      break;
    case "loopIn":
      loopIn(d);
      break;
    case "loopOut":
      loopOut(d);
      break;
    case "reloop":
      reloop(d);
      break;
    case "loopResize":
      resizeLoop(v, c.double);
      break;
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
      leaveLoopUnless(v, v.positionMs);
      break;
    case "callCue": {
      const at = Math.min(v.durationMs, Math.max(0, c.ms));
      v.cuePreview = false;
      leaveLoopUnless(v, at);
      v.cueMs = at;
      v.positionMs = at;
      const end = c.endMs == null ? null : Math.min(v.durationMs, c.endMs);
      if (end !== null && end - at >= MIN_LOOP_MS) v.lastLoop = makeLoop(d, at, end);
      break;
    }
    case "hotCue": {
      const at =
        v.trackId === null
          ? undefined
          : trackCues.get(v.trackId)?.hotCues.find((h) => h.slot === c.slot)?.positionMs;
      if (at !== undefined) {
        v.cuePreview = false;
        leaveLoopUnless(v, at);
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
    case "masterTempo":
      v.masterTempo = c.on;
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
    masterTempo: prev?.view.masterTempo ?? false,
  };
  d.grid = analysed;
  // A new track on the master hands the role on at the next tick.
  if (master === deck) masterPlaying = true;
  const cues = cuesOf(id, analysed);
  decks[deck] = d;
  return { deck, trackId: id, durationMs: DURATION_MS, ...cues };
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
      // Four mixer decks and the preview deck (`PREVIEW_DECK`).
      decks = Array.from({ length: PREVIEW_DECK + 1 }, emptyDeck);
      master = null;
      masterPlaying = false;
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
    case "mix_load_stick": {
      const deck = args.deck as number;
      // Stick tracks keep their cues under negative ids, apart from library tracks.
      const loaded = load(deck, -(args.trackId as number), true);
      const d = decks[deck];
      if (d !== undefined) d.view = { ...d.view, trackId: null };
      return new Promise((resolve) => setTimeout(() => resolve({ ...loaded, trackId: null }), 300));
    }
    case "mix_cue_edit": {
      const d = decks[args.deck as number];
      const id = d?.view.trackId;
      if (d?.view.loaded === true && id === null) {
        throw new Error("The cues of a track on a stick can't be changed here.");
      }
      if (d === undefined || id == null) throw new Error("the deck is empty");
      return editCues(id, args.edit as CueEdit, snap(d, d.view.positionMs), d.view.loopRegion);
    }
    case "mix_control":
      if (config === null) throw new Error("the mixer's audio output is not started");
      control(args.control as MixControl);
      return null;
    default:
      return undefined;
  }
}
