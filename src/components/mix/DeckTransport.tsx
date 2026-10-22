/** A deck's CUE (held: preview on the cue point) and PLAY, round like a CDJ's. */
import { PauseIcon, PlayIcon } from "../icons";
import { IconButton } from "../ui/IconButton";
import { holdHandlers } from "./hold";

export interface DeckTransportProps {
  deck: number;
  loaded: boolean;
  playing: boolean;
  cueKey: string | undefined;
  playKey: string | undefined;
  onCue: (down: boolean) => void;
  onTogglePlay: () => void;
}

export function DeckTransport({
  deck,
  loaded,
  playing,
  cueKey,
  playKey,
  onCue,
  onTogglePlay,
}: DeckTransportProps) {
  const n = deck + 1;
  return (
    <div className="flex w-(--deck-transport) shrink-0 flex-col justify-center gap-2">
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
