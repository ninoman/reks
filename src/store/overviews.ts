/**
 * Waveform thumbnails for the track table (`waveform_overview`). Rows ask
 * while they are on screen and let go when they scroll away; one request runs
 * at a time, newest first, because a track without cached columns is decoded
 * in Rust (seconds) and a fast scroll must not queue the whole library.
 */
import { waveformOverview } from "../lib/ipc";

/** Columns per thumbnail; the canvas resamples them to its width. */
export const OVERVIEW_WIDTH = 400;
const CACHE_MAX = 2000;

/** id → columns (null: unreadable, not asked again). Map order = recency. */
const cache = new Map<number, Uint8Array | null>();
/** Ids rows are waiting for, with how many rows wait. */
const wanted = new Map<number, number>();
const listeners = new Map<number, Set<() => void>>();
let running = false;

/** The cached thumbnail; `undefined` while not loaded. */
export function overviewOf(id: number): Uint8Array | null | undefined {
  return cache.get(id);
}

/** Listens for `id`'s thumbnail and asks for it until the returned function is called. */
export function watchOverview(id: number, onLoad: () => void): () => void {
  const set = listeners.get(id) ?? new Set();
  listeners.set(id, set);
  set.add(onLoad);
  if (!cache.has(id)) {
    // Re-inserted last: the newest rows are served first.
    const waiting = wanted.get(id) ?? 0;
    wanted.delete(id);
    wanted.set(id, waiting + 1);
    void pump();
  }
  return () => {
    set.delete(onLoad);
    if (set.size === 0) listeners.delete(id);
    const waiting = (wanted.get(id) ?? 1) - 1;
    if (waiting <= 0) wanted.delete(id);
    else wanted.set(id, waiting);
  };
}

async function pump() {
  if (running) return;
  running = true;
  try {
    for (let id = newest(); id !== undefined; id = newest()) {
      wanted.delete(id);
      const columns = await waveformOverview(id, OVERVIEW_WIDTH).then(
        (b) => new Uint8Array(b),
        () => null,
      );
      store(id, columns);
      for (const f of listeners.get(id) ?? []) f();
    }
  } finally {
    running = false;
  }
}

function newest(): number | undefined {
  return [...wanted.keys()].pop();
}

function store(id: number, columns: Uint8Array | null) {
  cache.set(id, columns);
  if (cache.size > CACHE_MAX) {
    const [oldest] = cache.keys();
    if (oldest !== undefined) cache.delete(oldest);
  }
}

/** Test hook: forget every thumbnail and request. */
export function clearOverviews(): void {
  cache.clear();
  wanted.clear();
  listeners.clear();
}
