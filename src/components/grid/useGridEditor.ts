/**
 * The grid editor's data and edits (phase 3 task 18): loads the grid, the
 * waveform columns and (with snap on) the transients, sends every edit to
 * Rust, and keeps the session's undo/redo history and the selected section.
 * The grid maths are in Rust (`reks-core` `beatgrid`); this only forwards.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import {
  EMPTY_HISTORY,
  type GridHistory,
  recordEdit,
  redoStep,
  sameAnchors,
  undoStep,
} from "../../lib/gridHistory";
import {
  type Beatgrid,
  beatgridEdit,
  beatgridGet,
  type GridEdit,
  waveformColumns,
  waveformTransients,
} from "../../lib/ipc";

export interface GridEditor {
  grid: Beatgrid | null;
  columns: Uint8Array | null;
  /** Loaded while snap is on; null before. */
  transients: number[] | null;
  error: string | null;
  busy: boolean;
  /** Index of the selected section (a marker), or null without a grid. */
  selected: number | null;
  select: (section: number) => void;
  snap: boolean;
  setSnap: (on: boolean) => void;
  canUndo: boolean;
  canRedo: boolean;
  /** Sends `edit`; edits that change the markers can be undone. */
  apply: (edit: GridEdit) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
}

export function useGridEditor(trackId: number, onChanged: () => void): GridEditor {
  const [grid, setGrid] = useState<Beatgrid | null>(null);
  const [columns, setColumns] = useState<Uint8Array | null>(null);
  const [transients, setTransients] = useState<number[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<number | null>(null);
  const [snap, setSnap] = useState(false);
  const [history, setHistory] = useState<GridHistory>(EMPTY_HISTORY);
  // One edit at a time: a key held down must not race its own results.
  const sending = useRef(false);

  useEffect(() => {
    let live = true;
    beatgridGet(trackId).then(
      (g) => live && setGrid(g),
      (e) => live && setError(String(e)),
    );
    // Computed on demand the first time (a few seconds for a long track).
    waveformColumns(trackId).then(
      (b) => live && setColumns(new Uint8Array(b)),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [trackId]);

  useEffect(() => {
    if (!snap || transients !== null) return;
    let live = true;
    waveformTransients(trackId).then(
      (t) => live && setTransients(t),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [snap, transients, trackId]);

  // Keep a selection inside the grid as markers come and go.
  const count = grid?.anchors.length ?? 0;
  useEffect(() => {
    setSelected((s) => (count === 0 ? null : Math.min(s ?? 0, count - 1)));
  }, [count]);

  const send = useCallback(
    async (
      edit: GridEdit,
      nextHistory: (before: Beatgrid, after: Beatgrid) => GridHistory | null,
    ) => {
      if (sending.current || grid === null) return;
      sending.current = true;
      setBusy(true);
      setError(null);
      try {
        const after = await beatgridEdit(trackId, edit, snap);
        const h = nextHistory(grid, after);
        if (h !== null) setHistory(h);
        // Follow a marker the edit added.
        if (edit.op === "addMarker") {
          const i = after.anchors.findIndex(
            (a) => !grid.anchors.some((b) => b.positionMs === a.positionMs),
          );
          if (i >= 0) setSelected(i);
        }
        setGrid(after);
        onChanged();
      } catch (e) {
        setError(String(e));
      } finally {
        sending.current = false;
        setBusy(false);
      }
    },
    [trackId, snap, grid, onChanged],
  );

  const apply = useCallback(
    (edit: GridEdit) =>
      send(edit, (before, after) =>
        sameAnchors(before.anchors, after.anchors) ? null : recordEdit(history, before.anchors),
      ),
    [send, history],
  );

  const undo = useCallback(async () => {
    const step = grid === null ? null : undoStep(history, grid.anchors);
    if (step !== null) await send(step.edit, () => step.history);
  }, [send, history, grid]);

  const redo = useCallback(async () => {
    const step = grid === null ? null : redoStep(history, grid.anchors);
    if (step !== null) await send(step.edit, () => step.history);
  }, [send, history, grid]);

  return {
    grid,
    columns,
    transients,
    error,
    busy,
    selected,
    select: setSelected,
    snap,
    setSnap,
    canUndo: history.past.length > 0,
    canRedo: history.future.length > 0,
    apply,
    undo,
    redo,
  };
}
