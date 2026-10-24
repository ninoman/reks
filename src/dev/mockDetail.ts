/**
 * Dev-only close-zoom detail for the mock (`mockMix.ts`): the points
 * reks-analysis `detail` would build from the generated fixture every mock
 * track shows (`mockGrid.ts`): a 440 Hz sine at 0.25 with a 10 ms, 2 kHz
 * click at 0.6 on every beat at 128 BPM. Bands are rough stand-ins.
 */
import { DETAIL_POINT_BYTES, DETAIL_POINTS_PER_SECOND } from "../lib/ipc";

const RATE = 44_100;
const BEAT_S = 60 / 128;
const CLICK_S = 0.01;
/** Samples a point spans here (a stand-in for the real 14.7). */
const SUB = 15;

function sample(t: number): number {
  const sine = 0.25 * Math.sin(2 * Math.PI * 440 * t);
  const since = t - Math.floor(t / BEAT_S) * BEAT_S;
  const click =
    since < CLICK_S ? 0.6 * (1 - since / CLICK_S) * Math.sin(2 * Math.PI * 2000 * since) : 0;
  return sine + click;
}

/** Points `first..first + count` of a track `durationMs` long. */
export function mockDetail(first: number, count: number, durationMs: number): ArrayBuffer {
  const total = Math.ceil((durationMs * DETAIL_POINTS_PER_SECOND) / 1000);
  const n = Math.max(0, Math.min(count, total - first));
  const out = new Uint8Array(n * DETAIL_POINT_BYTES);
  const signed = new Int8Array(out.buffer);
  for (let i = 0; i < n; i++) {
    const t0 = (first + i) / DETAIL_POINTS_PER_SECOND;
    let min = 1;
    let max = -1;
    for (let s = 0; s < SUB; s++) {
      const v = sample(t0 + s / RATE);
      min = Math.min(min, v);
      max = Math.max(max, v);
    }
    const since = t0 - Math.floor(t0 / BEAT_S) * BEAT_S;
    const o = i * DETAIL_POINT_BYTES;
    signed[o] = Math.round(min * 127);
    signed[o + 1] = Math.round(max * 127);
    out[o + 2] = 64;
    out[o + 3] = since < CLICK_S + 0.005 ? 153 : 0;
    out[o + 4] = since < CLICK_S + 0.005 ? 40 : 0;
  }
  return out.buffer;
}
