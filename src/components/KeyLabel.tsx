/** A key in the notation chosen in Settings (Classical, Camelot, Open Key). */
import type { Key } from "../lib/ipc";
import { useKeyLabel } from "../store/settings";

export function KeyLabel({ value }: { value: Key }) {
  return <>{useKeyLabel()(value)}</>;
}
