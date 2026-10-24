/**
 * Dev-only grid editor backend for the mock (`mockBackend.ts`). Every track
 * shows the real columns of the generated fixture `fixtures/audio/sine-click-128.wav`
 * (30 s, a click on every beat at 128 BPM from 0 ms), written by
 * `cargo run -p reks-cli -- columns fixtures/audio/sine-click-128.wav src/dev/fixtures/sine-click-128.columns.bin`.
 * Analysed tracks start with a 128 BPM grid 12 ms late, so nudging has
 * something to fix. The edits are a rough stand-in for `reks-core`
 * `beatgrid/`, which is the real (tested) implementation.
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

const mod = (n: number, bpb: number) => (((n % bpb) + bpb) % bpb) + 1;

/** Rough stand-in for Rust's beat rule: a section stops half a beat before the next marker. */
function beats(anchors: GridAnchor[]): BeatMark[] {
  const out: BeatMark[] = [];
  anchors.forEach((a, i) => {
    const period = 6_000_000 / a.bpmX100;
    const next = anchors[i + 1]?.positionMs ?? Infinity;
    const first = i === 0 ? -Math.floor(a.positionMs / period) : 0;
    for (let n = first; ; n++) {
      const ms = a.positionMs + n * period;
      if (ms > DURATION_MS || next - ms < period / 2) break;
      out.push({
        positionUs: Math.round(ms * 1000),
        beatInBar: mod(a.beatInBar - 1 + n, a.beatsPerBar),
      });
    }
  });
  return out;
}

function view(g: MockGrid): Beatgrid {
  return { ...g, beats: beats(g.anchors) };
}

/** The section holding `ms` and the beat number in it nearest `ms`. */
function nearest(anchors: GridAnchor[], ms: number): [number, number] {
  let i = 0;
  while (i + 1 < anchors.length && (anchors[i + 1]?.positionMs ?? Infinity) <= ms) i++;
  const a = anchors[i] as GridAnchor;
  return [i, Math.round((ms - a.positionMs) / (6_000_000 / a.bpmX100))];
}

function edit(g: MockGrid, e: GridEdit): MockGrid {
  if (e.op === "setLocked") return { ...g, locked: e.locked };
  if (g.locked) throw new Error("the beatgrid is locked");
  if (g.anchors.length === 0) {
    if (e.op !== "setBpm") throw new Error("track has no beatgrid");
    const anchors = [{ positionMs: e.anchorMs, bpmX100: e.bpmX100, beatInBar: 1, beatsPerBar: 4 }];
    return { ...g, anchors, source: "manual" };
  }
  const anchors = g.anchors.map((a) => ({ ...a }));
  const at = (i: number) => anchors[i] as GridAnchor;
  const each = (s: number | null) => (s === null ? anchors.map((_, i) => i) : [s]);
  switch (e.op) {
    case "nudge":
      for (const a of anchors) a.positionMs += e.deltaMs;
      break;
    case "setDownbeat": {
      const [i, n] = nearest(anchors, e.atMs);
      at(i).beatInBar = mod(-n, at(i).beatsPerBar);
      break;
    }
    case "double":
      for (const i of each(e.section)) {
        at(i).bpmX100 *= 2;
        at(i).beatInBar = mod(2 * (at(i).beatInBar - 1), at(i).beatsPerBar);
      }
      break;
    case "halve":
      for (const i of each(e.section)) at(i).bpmX100 = Math.round(at(i).bpmX100 / 2);
      break;
    case "setBpm":
      at(nearest(anchors, e.anchorMs)[0]).bpmX100 = e.bpmX100;
      break;
    case "setSectionBpm":
      at(e.section).bpmX100 = e.bpmX100;
      break;
    case "addMarker": {
      const [i, n] = nearest(anchors, e.atMs);
      const a = at(i);
      anchors.splice(i + 1, 0, {
        ...a,
        positionMs: e.atMs,
        beatInBar: mod(a.beatInBar - 1 + n, a.beatsPerBar),
      });
      break;
    }
    case "moveMarker":
      at(e.section).positionMs = e.toMs;
      break;
    case "deleteMarker":
      if (anchors.length < 2) throw new Error("a grid keeps at least one marker");
      anchors.splice(e.section, 1);
      break;
    case "stretch": {
      const a = at(e.section);
      const n = Math.round((e.beatMs - a.positionMs) / (6_000_000 / a.bpmX100));
      if (n === 0) throw new Error("pick a beat other than the marker");
      a.bpmX100 = Math.round((n * 6_000_000) / (e.toMs - a.positionMs));
      break;
    }
    case "reset":
      return initial(true);
    case "setAnchors":
      return { ...g, anchors: e.anchors, source: "manual" };
  }
  return { ...g, anchors, source: "manual" };
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
    // The fixture's clicks: one on every beat at 128 BPM from 0 ms.
    case "waveform_transients":
      return Array.from({ length: 64 }, (_, n) => Math.round(n * 468.75));
    case "waveform_columns":
    case "device_waveform_columns":
      columns ??= fetch(columnsUrl).then((r) => r.arrayBuffer());
      return columns.then((b) => b.slice(0));
    case "waveform_overview":
      columns ??= fetch(columnsUrl).then((r) => r.arrayBuffer());
      return columns.then((b) => downsample(new Uint8Array(b), args.width as number));
    default:
      return undefined;
  }
}

/** Run peaks down to `width` columns, as reks-analysis `columns::downsample`. */
function downsample(cols: Uint8Array, width: number): ArrayBuffer {
  const n = cols.length / 4;
  if (n <= width) return cols.slice().buffer;
  const out = new Uint8Array(width * 4);
  for (let x = 0; x < width; x++) {
    for (let c = Math.floor((x * n) / width); c < Math.floor(((x + 1) * n) / width); c++) {
      for (let k = 0; k < 4; k++)
        out[4 * x + k] = Math.max(out[4 * x + k] ?? 0, cols[4 * c + k] ?? 0);
    }
  }
  return out.buffer;
}
