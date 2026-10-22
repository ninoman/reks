/**
 * Dev-only grid editor backend for the mock (`mockBackend.ts`). Every track
 * shows the real columns of the generated fixture `fixtures/audio/sine-click-128.wav`
 * (30 s, a click on every beat at 128 BPM from 0 ms), written by
 * `cargo run -p reks-cli -- columns fixtures/audio/sine-click-128.wav src/dev/fixtures/sine-click-128.columns.bin`.
 * Analysed tracks start with a 128 BPM grid 12 ms late, so nudging has
 * something to fix. The edits are a rough stand-in for `reks-core`
 * `beatgrid.rs`, which is the real (tested) implementation.
 */

import type { Beatgrid, BeatMark, GridAnchor, GridEdit } from "../lib/ipc";
import columnsUrl from "./fixtures/sine-click-128.columns.bin?url";

const DURATION_MS = 30_000;

interface MockGrid {
  anchors: GridAnchor[];
  source: string | null;
  locked: boolean;
}

const grids = new Map<number, MockGrid>();
let columns: Promise<ArrayBuffer> | null = null;

function initial(analysed: boolean): MockGrid {
  return analysed
    ? {
        anchors: [{ positionMs: 12, bpmX100: 12_800, beatInBar: 1, beatsPerBar: 4 }],
        source: "analysis",
        locked: false,
      }
    : { anchors: [], source: null, locked: false };
}

function beats(anchors: GridAnchor[]): BeatMark[] {
  const out: BeatMark[] = [];
  const a = anchors[0];
  if (a === undefined) return out;
  const period = 6_000_000_000 / a.bpmX100;
  for (let n = 0; ; n++) {
    const pos = a.positionMs * 1000 + Math.round(n * period);
    if (pos > DURATION_MS * 1000) break;
    out.push({ positionUs: pos, beatInBar: ((a.beatInBar - 1 + n) % a.beatsPerBar) + 1 });
  }
  return out;
}

function view(g: MockGrid): Beatgrid {
  return { ...g, beats: beats(g.anchors) };
}

function edit(g: MockGrid, e: GridEdit): MockGrid {
  if (e.op === "setLocked") return { ...g, locked: e.locked };
  if (g.locked) throw new Error("the beatgrid is locked");
  const a = g.anchors[0];
  if (a === undefined) {
    if (e.op !== "setBpm") throw new Error("track has no beatgrid");
    const anchors = [{ positionMs: e.anchorMs, bpmX100: e.bpmX100, beatInBar: 1, beatsPerBar: 4 }];
    return { ...g, anchors, source: "manual" };
  }
  const period = 6_000_000 / a.bpmX100;
  const mod = (n: number) => (((n % 4) + 4) % 4) + 1;
  let next: GridAnchor;
  switch (e.op) {
    case "nudge":
      next = { ...a, positionMs: a.positionMs + e.deltaMs };
      break;
    case "setDownbeat":
      next = { ...a, beatInBar: mod(-Math.round((e.atMs - a.positionMs) / period)) };
      break;
    case "double":
      next = { ...a, bpmX100: a.bpmX100 * 2, beatInBar: mod(2 * (a.beatInBar - 1)) };
      break;
    case "halve":
      next = { ...a, bpmX100: Math.round(a.bpmX100 / 2) };
      break;
    case "setBpm":
      next = { ...a, bpmX100: e.bpmX100 };
      break;
  }
  return { ...g, anchors: [next], source: "manual" };
}

export function mockGrid(
  cmd: string,
  args: Record<string, unknown>,
  analysed: (id: number) => boolean,
): unknown {
  const id = args.id as number;
  const current = () => grids.get(id) ?? initial(analysed(id));
  switch (cmd) {
    case "beatgrid_get":
      return view(current());
    // Stick tracks carry the stick's analysis: always the analysed fixture.
    case "device_beatgrid":
      return { ...view(initial(true)), source: "stick", locked: true };
    case "beatgrid_edit": {
      const g = edit(current(), args.edit as GridEdit);
      grids.set(id, g);
      return view(g);
    }
    case "beatgrid_tap_tempo": {
      const taps = args.tapsMs as number[];
      if (taps.length < 2) return null;
      const first = taps[0] ?? 0;
      const last = taps[taps.length - 1] ?? 0;
      return Math.round(6_000_000 / ((last - first) / (taps.length - 1)));
    }
    case "waveform_columns":
    case "device_waveform_columns":
      columns ??= fetch(columnsUrl).then((r) => r.arrayBuffer());
      return columns.then((b) => b.slice(0));
    default:
      return undefined;
  }
}
