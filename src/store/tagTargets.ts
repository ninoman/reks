/**
 * What the My Tags drawer's checkboxes act on: the main list's selected
 * tracks whose pages are loaded. Follows the selection live, like the info
 * panel, so picking another track while the drawer is open retargets it.
 */
import { useMemo } from "react";

import type { TrackRow } from "../lib/ipc";
import { type LoadedView, rowAt, useLibrary } from "./library";

/** Loaded track rows at the `selected` table indices, in table order. */
export function selectedRows(v: LoadedView, selected: ReadonlySet<number>): TrackRow[] {
  return [...selected]
    .sort((a, b) => a - b)
    .flatMap((i) => {
      const row = rowAt(v, i);
      return row === undefined ? [] : [row];
    });
}

export function useTagTargets(): TrackRow[] {
  const selected = useLibrary((s) => s.selection.selected);
  const pages = useLibrary((s) => s.pages);
  const total = useLibrary((s) => s.total);
  const dividers = useLibrary((s) => s.dividers);
  return useMemo(
    () => selectedRows({ pages, total, dividers }, selected),
    [selected, pages, total, dividers],
  );
}
