/**
 * What sits on a deck's zoomed waveform: the sync phase meter along the top
 * edge (synced decks), zoom − / + with the zoom level at the top right
 * (shown on hover or keyboard focus), and a warning inside the bottom edge.
 */
import { DECK_ZOOM_S, zoomSeconds } from "../../lib/deckScroll";
import { MinusIcon, PlusIcon } from "../icons";
import { IconButton } from "../ui/IconButton";
import { SyncPhase } from "./SyncPhase";

export interface DeckWaveformRowProps {
  deck: number;
  zoom: number;
  zoomOutKey: string | undefined;
  zoomInKey: string | undefined;
  onZoom: (dir: -1 | 1) => void;
  /** Show the phase meter (synced, not the master). */
  phaseShown: boolean;
  /** Beats from the master's phase; null when there is none to compare. */
  phase: number | null;
  warning: string | null;
}

export function DeckWaveformRow({
  deck,
  zoom,
  zoomOutKey,
  zoomInKey,
  onZoom,
  phaseShown,
  phase,
  warning,
}: DeckWaveformRowProps) {
  const n = deck + 1;
  return (
    <>
      {phaseShown && (
        <div className="pointer-events-none absolute top-0.5 left-1/2 -translate-x-1/2">
          <SyncPhase offset={phase} />
        </div>
      )}
      <div className="absolute top-1 right-1 flex items-center gap-0.5 rounded-xs bg-overlay p-0.5 opacity-0 focus-within:opacity-100 group-hover/wave:opacity-100">
        <IconButton
          size="xs"
          variant="ghost"
          aria-label={`Deck ${n} zoom out`}
          hint="Zoom out"
          shortcut={zoomOutKey}
          disabled={zoom <= 0}
          onClick={() => onZoom(-1)}
          className="text-deck-fg hover:bg-deck hover:text-deck-fg"
        >
          <MinusIcon />
        </IconButton>
        <span className="px-0.5 font-mono text-2xs text-deck-fg tabular-nums">
          {zoomSeconds(zoom)} s
        </span>
        <IconButton
          size="xs"
          variant="ghost"
          aria-label={`Deck ${n} zoom in`}
          hint="Zoom in"
          shortcut={zoomInKey}
          disabled={zoom >= DECK_ZOOM_S.length - 1}
          onClick={() => onZoom(1)}
          className="text-deck-fg hover:bg-deck hover:text-deck-fg"
        >
          <PlusIcon />
        </IconButton>
      </div>
      {warning !== null && (
        <p className="pointer-events-none absolute inset-x-1.5 bottom-0.5 truncate text-center text-2xs text-warning">
          {warning}
        </p>
      )}
    </>
  );
}
