/** Tap tempo for the grid editor: the newest taps go to Rust, which fits the tempo. */
import { useCallback, useState } from "react";

import { beatgridTapTempo } from "../../lib/ipc";

/** Taps kept for the tempo (Rust uses the newest run of these). */
const MAX_TAPS = 16;

export function useTap(): { bpmX100: number | null; tap: () => Promise<void> } {
  const [taps, setTaps] = useState<number[]>([]);
  const [bpmX100, setBpm] = useState<number | null>(null);
  const tap = useCallback(async () => {
    const next = [...taps, performance.now()].slice(-MAX_TAPS);
    setTaps(next);
    setBpm(await beatgridTapTempo(next));
  }, [taps]);
  return { bpmX100, tap };
}
