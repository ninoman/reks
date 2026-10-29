/**
 * Connects grid mode (`store/deckGrid.ts`) to the decks: for each deck, the
 * GRID toggle, whether the grid keys reach it, and, on the deck in grid
 * mode, the panel's props and what the waveform draws over the track. One
 * call per host (the Mix panel, the player deck); only one deck is in grid
 * mode, so one tap tempo serves it.
 */
import {
  barBeatAt,
  formatBarBeat,
  formatOffset,
  pivotMs,
  removableMarker,
} from "../../lib/gridMode";
import { sectionAt } from "../../lib/gridView";
import type { DeckView } from "../../lib/ipc";
import { useDeckGrid } from "../../store/deckGrid";
import { type DeckMeta, libraryId } from "../../store/mix";
import type { DeckGridPanelProps, DeckGridState } from "./DeckGridPanel";
import { useTap } from "./useTap";

/** Drawn over the deck's zoomed waveform in grid mode. */
export interface GridOverlay {
  /** Tempo section markers: where each starts and its BPM. */
  markers: { ms: number; label: string }[];
  /** The beat stretch and BPM edits pivot on. */
  pivotMs: number | null;
  /** A marker dragged to `toMs`; `exact`: Alt held, no snap. */
  onMoveMarker: (section: number, toMs: number, exact: boolean) => void;
}

export interface DeckGridMode {
  /** A library track is loaded: grid mode can open. */
  available: boolean;
  on: boolean;
  /** The grid keys act on this deck. */
  focused: boolean;
  toggle: () => void;
  panel: DeckGridPanelProps | null;
  overlay: GridOverlay | null;
}

export function useDeckGridModes(): (
  deck: number,
  view: DeckView | undefined,
  meta: DeckMeta,
) => DeckGridMode {
  const g = useDeckGrid();
  const { bpmX100: tapBpmX100, tap } = useTap();

  return (deck, view, meta) => {
    const trackId = view?.loaded === true ? libraryId(meta.track) : null;
    const on = g.deck === deck && trackId !== null && g.trackId === trackId;
    const toggle = () => {
      if (on) g.close();
      else if (trackId !== null) void g.open(deck, trackId);
    };
    const base = { available: trackId !== null, on, focused: on && g.focus === deck, toggle };
    if (!on) return { ...base, panel: null, overlay: null };

    const at = view?.positionMs ?? 0;
    const grid = g.grid;
    const anchors = grid?.anchors ?? [];
    const state: DeckGridState =
      grid === null
        ? "loading"
        : grid.locked
          ? "locked"
          : anchors.length === 0
            ? "noGrid"
            : "ready";
    const here = barBeatAt(grid?.beats ?? [], at);
    const section = sectionAt(anchors, at);
    const panel: DeckGridPanelProps = {
      deck,
      state,
      position: here === null ? null : formatBarBeat(here),
      offset: here === null ? null : formatOffset(here.offsetMs),
      bpmX100: section === null ? null : (anchors[section]?.bpmX100 ?? null),
      onMarker: removableMarker(anchors, at) !== null,
      manual: grid?.source === "manual",
      canUndo: g.past.length > 0,
      canRedo: g.future.length > 0,
      tapBpmX100,
      busy: g.busy,
      error: g.error,
      actions: {
        shift: (dir, phase, big) => g.shift(at, dir, phase, big),
        bpmStep: (dir, phase, big) => g.bpmStep(at, dir, phase, big),
        setBpm: (bpm) => g.setBpm(at, bpm),
        beatOne: (exact) => g.beatOne(at, !exact),
        stretch: (exact) => g.stretch(at, !exact),
        marker: (exact) => g.marker(at, !exact),
        halve: () => g.halve(at),
        double: () => g.double(at),
        undo: g.undo,
        redo: g.redo,
        tap: () => void tap(),
        useTap: () => tapBpmX100 !== null && g.setBpm(at, tapBpmX100 / 100),
        reset: g.reset,
        setLocked: g.setLocked,
        exit: g.close,
      },
    };
    const overlay: GridOverlay = {
      markers: anchors.map((a) => ({ ms: a.positionMs, label: (a.bpmX100 / 100).toFixed(2) })),
      pivotMs: anchors.length === 0 ? null : pivotMs(anchors, g.pinMs, at),
      onMoveMarker: (s, toMs, exact) => g.moveMarker(s, toMs, !exact),
    };
    return { ...base, panel, overlay };
  };
}
