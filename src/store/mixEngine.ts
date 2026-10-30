/**
 * A page reload (the error screen's Reload, or a dev refresh) resets the mix
 * store to "stopped" while Rust's engine keeps playing. The deck state
 * lives in the store, so there is nothing to re-attach to: on mount the
 * engine is stopped too (`mix_stop` is a no-op when it isn't running),
 * and the UI never says "stopped" over live audio.
 */
import { useMix } from "./mix";

export function releaseStaleEngine(): Promise<void> {
  return useMix.getState().stop();
}
