/**
 * Waveform thumbnails for the track table (`waveform_overview`). Rows ask
 * while they are on screen and let go when they scroll away; newest rows are
 * served first. Two lanes: up to `FAST_LANES` requests read columns already on
 * disk (the columns cache, reks's or rekordbox's analysis; milliseconds), and
 * one decodes a track that has none (seconds), so a track without analysis
 * never holds up the rest and a fast scroll does not queue the whole library.
 */
import { waveformOverview } from "../lib/ipc";

const CACHE_MAX = 2000;
const FAST_LANES = 4;

/** id → columns (null: unreadable, not asked again). Map order = recency. */
const cache = new Map<number, Uint8Array | null>();
/** Ids rows are waiting for, with how many rows wait. Map order = recency. */
const wanted = new Map<number, number>();
const listeners = new Map<number, Set<() => void>>();
/** Ids with nothing on disk: only the decode lane serves them. */
const misses = new Set<number>();
const inFlight = new Set<number>();
let fastRunning = 0;
let decoding = false;

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
    pump();
  }
  return () => {
    set.delete(onLoad);
    if (set.size === 0) listeners.delete(id);
    const waiting = (wanted.get(id) ?? 1) - 1;
    if (waiting <= 0) wanted.delete(id);
    else wanted.set(id, waiting);
  };
}

function pump() {
  for (let id = nextFast(); id !== undefined; id = nextFast()) void load(id, false);
  if (!decoding) {
    const id = newest(true);
    if (id !== undefined) void load(id, true);
  }
}

function nextFast(): number | undefined {
  return fastRunning < FAST_LANES ? newest(false) : undefined;
}

/** The newest wanted id not in flight that needs decoding (`decode`) or not. */
function newest(decode: boolean): number | undefined {
  const ids = [...wanted.keys()];
  for (let i = ids.length - 1; i >= 0; i--) {
    const id = ids[i];
    if (id !== undefined && !inFlight.has(id) && misses.has(id) === decode) return id;
  }
  return undefined;
}

async function load(id: number, decode: boolean) {
  inFlight.add(id);
  if (decode) decoding = true;
  else fastRunning++;
  const columns = await waveformOverview(id, decode).then(
    (b) => new Uint8Array(b),
    () => null,
  );
  inFlight.delete(id);
  if (decode) decoding = false;
  else fastRunning--;
  if (!decode && columns?.length === 0) {
    misses.add(id);
  } else {
    misses.delete(id);
    wanted.delete(id);
    store(id, columns);
    for (const f of listeners.get(id) ?? []) f();
  }
  pump();
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
  misses.clear();
}
