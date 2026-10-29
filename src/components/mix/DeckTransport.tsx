/**
 * A deck's Q (quantise, lit when on; needs a grid) beside the pad mode tabs,
 * over CUE (held: preview on the cue point) and PLAY, round like a CDJ's,
 * one beside each pad row (`--deck-ctl-rows`). They fill the column, so they
 * grow and shrink with the deck. Grid mode leaves Q out.
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
  /** False in grid mode: the row keeps its height without Q. */
  showQuantise?: boolean;
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
  showQuantise = true,
  cueKey,
  playKey,
  onCue,
  onTogglePlay,
  onQuantise,
}: DeckTransportProps) {
  const n = deck + 1;
  return (
    <div className="grid w-(--deck-transport-w) shrink-0 grid-rows-(--deck-ctl-rows) justify-items-center gap-y-1">
      {showQuantise ? (
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
      ) : (
        <span />
      )}
      <IconButton
        size="lg"
        shape="round"
        variant="control"
        disabled={!loaded}
        aria-label={`Deck ${n} cue`}
        hint="Cue (hold to preview)"
        shortcut={cueKey}
        className="size-full text-2xs font-bold"
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
        className="size-full"
        onClick={onTogglePlay}
      >
        {playing ? <PauseIcon /> : <PlayIcon />}
      </IconButton>
    </div>
  );
}
