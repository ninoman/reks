/**
 * A deck's Q (quantise, lit when on; needs a grid) over CUE (held: preview
 * on the cue point) and PLAY, round like a CDJ's, at the bottom of the column.
 * `compact` drops the column: the buttons place themselves in the parent's
 * two-column grid (CUE / PLAY on top, Q under CUE).
 */
import { cn } from "../../lib/cn";
import { PauseIcon, PlayIcon } from "../icons";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";
import { holdHandlers } from "./hold";

export interface DeckTransportProps {
  deck: number;
  loaded: boolean;
  playing: boolean;
  /** The engine has a grid: Q can act. */
  grid: boolean;
  quantise: boolean;
  cueKey: string | undefined;
  playKey: string | undefined;
  onCue: (down: boolean) => void;
  onTogglePlay: () => void;
  onQuantise: () => void;
  compact?: boolean;
}

export function DeckTransport({
  deck,
  loaded,
  playing,
  grid,
  quantise,
  cueKey,
  playKey,
  onCue,
  onTogglePlay,
  onQuantise,
  compact = false,
}: DeckTransportProps) {
  const n = deck + 1;
  const size = compact ? "md" : "lg";
  const buttons = (
    <>
      <Button
        size="xs"
        variant="control"
        disabled={!loaded || !grid}
        aria-pressed={quantise}
        aria-label={`Deck ${n} quantise`}
        hint="Quantise"
        className={cn(
          "w-(--ctl-h-xs) px-0",
          compact && "col-start-1 row-start-2 justify-self-center",
        )}
        onClick={onQuantise}
      >
        Q
      </Button>
      <IconButton
        size={size}
        shape="round"
        variant="control"
        disabled={!loaded}
        aria-label={`Deck ${n} cue`}
        hint="Cue (hold to preview)"
        shortcut={cueKey}
        className={cn("text-2xs font-bold", compact && "col-start-1 row-start-1")}
        {...holdHandlers(onCue)}
      >
        CUE
      </IconButton>
      <IconButton
        size={size}
        shape="round"
        variant="control"
        disabled={!loaded}
        aria-pressed={playing}
        aria-label={`Play deck ${n}`}
        hint={playing ? "Pause" : "Play"}
        shortcut={playKey}
        className={cn(compact && "col-start-2 row-start-1")}
        onClick={onTogglePlay}
      >
        {playing ? <PauseIcon /> : <PlayIcon />}
      </IconButton>
    </>
  );
  if (compact) return buttons;
  return (
    <div className="flex w-(--deck-transport) shrink-0 flex-col items-center justify-end gap-1">
      {buttons}
    </div>
  );
}
