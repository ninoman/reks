/** A track's waveform thumbnail, asked for while the calling row is mounted. */
import { useCallback, useSyncExternalStore } from "react";

import { overviewOf, watchOverview } from "../store/overviews";

/** `undefined` while loading or when `id` is null; null when unreadable. */
export function useOverview(id: number | null): Uint8Array | null | undefined {
  const subscribe = useCallback(
    (onChange: () => void) => (id === null ? () => {} : watchOverview(id, onChange)),
    [id],
  );
  return useSyncExternalStore(subscribe, () => (id === null ? undefined : overviewOf(id)));
}
