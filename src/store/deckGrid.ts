/**
 * Deck grid mode (D10, phase 5 task 32): one deck at a time adjusts its
 * track's grid on the deck itself. Holds that deck, the focused deck (the
 * grid keys act on it), the grid as Rust returned it, the pinned downbeat
 * and the session's undo / redo. Every edit goes to Rust (`mixGridEdit`),
 * which does the maths, stores the grid as `manual` and hands it to the
 * playing deck; the deck's beat lines follow (`useMix.setDeckBeats`).
 *
 * Edits go out one at a time. Steps that come in meanwhile (a held pad or
 * key) add up and go out as one edit; a hold is one undo step.
 */
import { create } from "zustand";

import {
  BPM_STEP_X100,
  type HoldPhase,
  pivotMs,
  removableMarker,
  SHIFT_STEP_MS,
} from "../lib/gridMode";
import { sectionAt } from "../lib/gridView";
import {
  type Beatgrid,
  beatgridGet,
  type GridAnchor,
  type GridEdit,
  mixGridEdit,
} from "../lib/ipc";
import { useMix } from "./mix";

export interface DeckGridStore {
  /** The deck in grid mode, or null. */
  deck: number | null;
  trackId: number | null;
  /** The deck the pointer was last on: the grid keys act on it. */
  focus: number | null;
  /** Null until fetched. */
  grid: Beatgrid | null;
  /** Pinned downbeat (ms); null: each section pivots on its marker. */
  pinMs: number | null;
  past: GridAnchor[][];
  future: GridAnchor[][];
  busy: boolean;
  error: string | null;
  /** Bumped per track on every stored edit, so views of the track refetch its grid summary. */
  revisions: Record<number, number>;

  /** Grid mode on `deck` for library track `trackId` (any other deck leaves it). */
  open: (deck: number, trackId: number) => Promise<void>;
  close: () => void;
  setFocus: (deck: number | null) => void;
  /** SHIFT ◀ ▶ at playhead `atMs`: the section there (the grid, with one section). */
  shift: (atMs: number, dir: -1 | 1, phase: HoldPhase, big: boolean) => void;
  /** BPM − / + of the section at `atMs`, about the pivot. */
  bpmStep: (atMs: number, dir: -1 | 1, phase: HoldPhase, big: boolean) => void;
  /** A typed BPM; with no grid yet, starts one with beat 1 at `atMs`. */
  setBpm: (atMs: number, bpm: number) => void;
  /** The nearest beat onto the playhead as beat 1; pins it. */
  beatOne: (atMs: number, snap: boolean) => void;
  /** The beat nearest the playhead onto it, the pivot fixed. */
  stretch: (atMs: number, snap: boolean) => void;
  /** Removes the marker at the playhead, else adds one there. */
  marker: (atMs: number, snap: boolean) => void;
  moveMarker: (section: number, toMs: number, snap: boolean) => void;
  halve: (atMs: number) => void;
  double: (atMs: number) => void;
  undo: () => void;
  redo: () => void;
  reset: () => void;
  setLocked: (locked: boolean) => void;
}

interface Job {
  /** Built when it goes out, from the grid as it is then; null skips it. */
  edit: (g: Beatgrid) => GridEdit | null;
  snap: boolean;
  /** Pushes the grid before it as one undo step. */
  record: boolean;
  /** Undo / redo keep their stacks themselves. */
  history?: "undo" | "redo";
  /** The pin after the edit, from the grid before and after it. */
  pin?: (pin: number | null, before: Beatgrid, after: Beatgrid) => number | null;
}

let queue: Promise<void> = Promise.resolve();
/** Steps of a SHIFT or BPM hold that have not gone out yet. */
let shiftAcc: { atMs: number; deltaMs: number; record: boolean } | null = null;
let bpmAcc: { atMs: number; stepX100: number; record: boolean } | null = null;
/** Bumped by open and close, so edits of an old session are dropped. */
let session = 0;

const sectionOf = (g: Beatgrid, atMs: number) => sectionAt(g.anchors, atMs) ?? 0;
/** Only that section when the grid has several; null shifts the whole grid. */
const shiftSection = (g: Beatgrid, atMs: number) =>
  g.anchors.length > 1 ? sectionOf(g, atMs) : null;

/** Position (ms) of the beat of `g` nearest `ms`. */
function nearestBeatMs(g: Beatgrid, ms: number): number {
  let best = ms;
  let d = Infinity;
  for (const b of g.beats) {
    const at = b.positionUs / 1000;
    if (Math.abs(at - ms) < d) {
      d = Math.abs(at - ms);
      best = at;
    }
  }
  return Math.round(best);
}

export const useDeckGrid = create<DeckGridStore>((set, get) => {
  function enqueue(job: Job) {
    const id = session;
    queue = queue.then(async () => {
      const s = get();
      if (id !== session || s.deck === null || s.grid === null) return;
      const deck = s.deck;
      const before = s.grid;
      const edit = job.edit(before);
      if (edit === null) return;
      set({ busy: true });
      try {
        const after = await mixGridEdit(deck, edit, job.snap);
        if (id !== session) return;
        const cur = get();
        set({
          grid: after,
          busy: false,
          error: null,
          pinMs: job.pin === undefined ? cur.pinMs : job.pin(cur.pinMs, before, after),
          past:
            job.history === "undo"
              ? cur.past.slice(0, -1)
              : job.record
                ? [...cur.past, before.anchors]
                : job.history === "redo"
                  ? [...cur.past, before.anchors]
                  : cur.past,
          future:
            job.history === "undo"
              ? [...cur.future, before.anchors]
              : job.history === "redo"
                ? cur.future.slice(0, -1)
                : job.record
                  ? []
                  : cur.future,
          revisions: cur.trackId === null ? cur.revisions : bump(cur.revisions, cur.trackId),
        });
        useMix.getState().setDeckBeats(deck, after.beats);
      } catch (e) {
        if (id === session) set({ busy: false, error: String(e) });
      }
    });
  }

  return {
    deck: null,
    trackId: null,
    focus: null,
    grid: null,
    pinMs: null,
    past: [],
    future: [],
    busy: false,
    error: null,
    revisions: {},

    async open(deck, trackId) {
      session++;
      const id = session;
      shiftAcc = null;
      bpmAcc = null;
      set({
        deck,
        trackId,
        focus: deck,
        grid: null,
        pinMs: null,
        past: [],
        future: [],
        busy: false,
        error: null,
      });
      try {
        const grid = await beatgridGet(trackId);
        if (id === session) set({ grid });
      } catch (e) {
        if (id === session) set({ error: String(e) });
      }
    },

    close() {
      session++;
      shiftAcc = null;
      bpmAcc = null;
      set({
        deck: null,
        trackId: null,
        grid: null,
        pinMs: null,
        past: [],
        future: [],
        busy: false,
        error: null,
      });
    },

    setFocus(focus) {
      if (get().focus !== focus) set({ focus });
    },

    shift(atMs, dir, phase, big) {
      const deltaMs = dir * (big || phase === "repeat" ? SHIFT_STEP_MS.hold : SHIFT_STEP_MS.tap);
      const record = phase === "press";
      if (shiftAcc !== null) {
        shiftAcc.deltaMs += deltaMs;
        shiftAcc.record ||= record;
        return;
      }
      shiftAcc = { atMs, deltaMs, record };
      const acc = shiftAcc;
      enqueue({
        edit: (g) => {
          shiftAcc = null;
          if (acc.deltaMs === 0) return null;
          return { op: "nudge", deltaMs: acc.deltaMs, section: shiftSection(g, acc.atMs) };
        },
        snap: false,
        get record() {
          return acc.record;
        },
        // The pin moves with its section.
        pin: (pin, before) => {
          if (pin === null) return null;
          const s = shiftSection(before, acc.atMs);
          return s === null || sectionOf(before, pin) === s ? pin + acc.deltaMs : pin;
        },
      });
    },

    bpmStep(atMs, dir, phase, big) {
      const stepX100 = dir * (big || phase === "repeat" ? BPM_STEP_X100.hold : BPM_STEP_X100.tap);
      const record = phase === "press";
      if (bpmAcc !== null) {
        bpmAcc.stepX100 += stepX100;
        bpmAcc.record ||= record;
        return;
      }
      bpmAcc = { atMs, stepX100, record };
      const acc = bpmAcc;
      enqueue({
        edit: (g) => {
          bpmAcc = null;
          const a = g.anchors[sectionOf(g, acc.atMs)];
          if (a === undefined || acc.stepX100 === 0) return null;
          return {
            op: "retempo",
            atMs: acc.atMs,
            pivotMs: pivotMs(g.anchors, get().pinMs, acc.atMs),
            bpmX100: a.bpmX100 + acc.stepX100,
          };
        },
        snap: false,
        get record() {
          return acc.record;
        },
      });
    },

    setBpm(atMs, bpm) {
      const bpmX100 = Math.round(bpm * 100);
      enqueue({
        edit: (g) =>
          g.anchors.length === 0
            ? { op: "setBpm", bpmX100, anchorMs: Math.round(atMs) }
            : { op: "retempo", atMs, pivotMs: pivotMs(g.anchors, get().pinMs, atMs), bpmX100 },
        snap: false,
        record: true,
      });
    },

    beatOne(atMs, snap) {
      const at = Math.round(atMs);
      enqueue({
        edit: () => ({ op: "beatOneAt", atMs: at }),
        snap,
        record: true,
        pin: (_, __, after) => nearestBeatMs(after, at),
      });
    },

    stretch(atMs, snap) {
      const to = Math.round(atMs);
      enqueue({
        edit: (g) => ({ op: "stretchTo", pivotMs: pivotMs(g.anchors, get().pinMs, to), toMs: to }),
        snap,
        record: true,
      });
    },

    marker(atMs, snap) {
      const at = Math.round(atMs);
      enqueue({
        edit: (g) => {
          const near = removableMarker(g.anchors, at);
          return near === null
            ? { op: "addMarker", atMs: at }
            : { op: "deleteMarker", section: near };
        },
        snap,
        record: true,
      });
    },

    moveMarker(section, toMs, snap) {
      enqueue({
        edit: () => ({ op: "moveMarker", section, toMs: Math.round(toMs) }),
        snap,
        record: true,
        // The pin moves with its section.
        pin: (pin, before, after) => {
          if (pin === null || sectionOf(before, pin) !== section) return pin;
          const from = before.anchors[section]?.positionMs ?? 0;
          return pin + (after.anchors[section]?.positionMs ?? from) - from;
        },
      });
    },

    halve(atMs) {
      enqueue({
        edit: (g) => ({ op: "halve", section: sectionOf(g, atMs) }),
        snap: false,
        record: true,
      });
    },

    double(atMs) {
      enqueue({
        edit: (g) => ({ op: "double", section: sectionOf(g, atMs) }),
        snap: false,
        record: true,
      });
    },

    undo() {
      const prev = get().past.at(-1);
      if (prev === undefined) return;
      enqueue({
        edit: () => ({ op: "setAnchors", anchors: prev }),
        snap: false,
        record: false,
        history: "undo",
      });
    },

    redo() {
      const next = get().future.at(-1);
      if (next === undefined) return;
      enqueue({
        edit: () => ({ op: "setAnchors", anchors: next }),
        snap: false,
        record: false,
        history: "redo",
      });
    },

    reset() {
      enqueue({
        edit: () => ({ op: "reset", section: null }),
        snap: false,
        record: true,
        pin: () => null,
      });
    },

    setLocked(locked) {
      enqueue({ edit: () => ({ op: "setLocked", locked }), snap: false, record: false });
    },
  };
});

function bump(revisions: Record<number, number>, trackId: number): Record<number, number> {
  return { ...revisions, [trackId]: (revisions[trackId] ?? 0) + 1 };
}
