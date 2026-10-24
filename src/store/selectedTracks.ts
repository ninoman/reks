/**
 * The tracks behind the table selection, for actions on all of them (bulk
 * edit). Unlike `useTagTargets` this loads the pages a selection spans, so a
 * select-all over several pages still covers every track: `pending` counts
 * the selected rows whose page has not arrived yet.
 */
import { useEffect, useMemo } from "react";

import type { TrackRow } from "../lib/ipc";
import { itemAt, useLibrary } from "./library";

export interface SelectedTracks {
  /** Loaded tracks (not dividers), once each, in table order. */
  rows: TrackRow[];
  pending: number;
}

export function useSelectedTracks(): SelectedTracks {
  const selected = useLibrary((s) => s.selection.selected);
  const pages = useLibrary((s) => s.pages);
  const total = useLibrary((s) => s.total);
  const dividers = useLibrary((s) => s.dividers);
  const ensureRange = useLibrary((s) => s.ensureRange);

  const out = useMemo(() => {
    const seen = new Set<number>();
    const rows: TrackRow[] = [];
    let pending = 0;
    for (const i of [...selected].sort((a, b) => a - b)) {
      const item = itemAt({ pages, total, dividers }, i);
      if (item === undefined) pending++;
      else if (item.kind === "track" && !seen.has(item.row.id)) {
        seen.add(item.row.id);
        rows.push(item.row);
      }
    }
    return { rows, pending };
  }, [selected, pages, total, dividers]);

  useEffect(() => {
    if (out.pending === 0) return;
    ensureRange(Math.min(...selected), Math.max(...selected));
  }, [out.pending, selected, ensureRange]);

  return out;
}
