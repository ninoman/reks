/**
 * A deck's Q (quantise, lit when on; needs a grid) over CUE (held: preview
 * on the cue point) and PLAY, round like a CDJ's, at the bottom of the column.
 */
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
}: DeckTransportProps) {
  const n = deck + 1;
  return (
    <div className="flex w-(--deck-transport) shrink-0 flex-col items-center justify-end gap-1">
      <Button
        size="xs"
        variant="control"
        disabled={!loaded || !grid}
        aria-pressed={quantise}
        aria-label={`Deck ${n} quantise`}
        hint="Quantise"
        className="w-(--ctl-h-xs) px-0"
        onClick={onQuantise}
      >
        Q
      </Button>
      <IconButton
        size="lg"
        shape="round"
        variant="control"
        disabled={!loaded}
        aria-label={`Deck ${n} cue`}
        hint="Cue (hold to preview)"
        shortcut={cueKey}
        className="text-2xs font-bold"
        {...holdHandlers(onCue)}
      >
        CUE
      </IconButton>
      <IconButton
        size="lg"
        shape="round"
        variant="control"
        disabled={!loaded}
        aria-pressed={playing}
        aria-label={`Play deck ${n}`}
        hint={playing ? "Pause" : "Play"}
        shortcut={playKey}
        onClick={onTogglePlay}
      >
        {playing ? <PauseIcon /> : <PlayIcon />}
      </IconButton>
    </div>
  );
}
