/**
 * Fetches a deck load's close-zoom detail (`deckDetail.ts`) one chunk at a
 * time, nearest the playhead first, and wakes again when the playhead moves
 * into another chunk (a long mix keeps only the chunks near it). The deck
 * plays and draws from the columns meanwhile.
 */
import { useEffect, useRef, useState } from "react";
import { CHUNK_POINTS, DetailChunks, pointAt } from "../../lib/deckDetail";
import { DETAIL_POINTS_PER_SECOND, mixWaveformDetail } from "../../lib/ipc";

export function useDeckDetail(
  deck: number,
  load: number | null,
  durationMs: number,
  positionMs: number,
): DetailChunks | null {
  const [chunks, setChunks] = useState<DetailChunks | null>(null);
  const position = useRef(positionMs);
  position.current = positionMs;
  const wake = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (load === null || durationMs <= 0) {
      setChunks(null);
      return;
    }
    const c = new DetailChunks((durationMs * DETAIL_POINTS_PER_SECOND) / 1000);
    setChunks(c);
    let live = true;
    void (async () => {
      while (live) {
        const next = c.next(pointAt(position.current));
        if (next === null) {
          await new Promise<void>((resolve) => {
            wake.current = resolve;
          });
          continue;
        }
        let bytes: ArrayBuffer;
        try {
          bytes = await mixWaveformDetail(deck, load, next * CHUNK_POINTS, CHUNK_POINTS);
        } catch {
          // The deck keeps the columns waveform.
          return;
        }
        // Empty: the deck holds another load by now.
        if (!live || bytes.byteLength === 0) return;
        c.add(next, new Uint8Array(bytes), pointAt(position.current));
      }
    })();
    return () => {
      live = false;
      wake.current?.();
      wake.current = null;
    };
  }, [deck, load, durationMs]);

  const chunk = Math.floor(pointAt(positionMs) / CHUNK_POINTS);
  // biome-ignore lint/correctness/useExhaustiveDependencies: wakes the loader when the playhead enters another chunk.
  useEffect(() => {
    const resolve = wake.current;
    wake.current = null;
    resolve?.();
  }, [chunk]);

  return chunks;
}
