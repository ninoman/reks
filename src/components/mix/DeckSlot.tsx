/**
 * What stands in for a deck's waveforms when it has none to show: empty
 * (how to load), loading, analysing (the deck plays meanwhile), or an
 * error with Retry and Eject. Same height as the two views.
 */
import { cn } from "../../lib/cn";
import { ReportErrorButton } from "../ErrorMessage";
import { Button } from "../ui/Button";
import { Progress } from "../ui/Progress";

export type DeckSlotState =
  | { kind: "empty" }
  | { kind: "loading" }
  | { kind: "analysing" }
  /** `playing`: only the waveform failed; the deck still plays, so no Eject. */
  | { kind: "error"; message: string; playing: boolean };

export interface DeckSlotProps {
  deck: number;
  state: DeckSlotState;
  /** The player deck: loaded by double-click. */
  solo: boolean;
  onRetry: () => void;
  onEject: () => void;
}

const box =
  "flex min-h-0 flex-1 flex-col items-center justify-center gap-1.5 rounded-xs border border-dashed border-border-strong px-3 text-center text-xs text-fg-muted";

export function DeckSlot({ deck, state, solo, onRetry, onEject }: DeckSlotProps) {
  const n = deck + 1;
  switch (state.kind) {
    case "empty":
      return (
        <div className={box}>
          {solo
            ? "Double-click a track to play it, or drag one here"
            : `Drag a track here (from the list or another deck), or right-click one and choose Load to deck ${n}`}
        </div>
      );
    case "loading":
      return (
        <div className={box} role="status">
          Decoding…
          <Progress size="sm" className="w-3/5" aria-label={`Deck ${n} decoding`} />
        </div>
      );
    case "analysing":
      return (
        <div className={box} role="status">
          Analysing the beatgrid…
          <Progress size="sm" className="w-3/5" aria-label={`Deck ${n} analysing`} />
          <span className="text-2xs">
            Playable now; BPM, sync, quantise, jumps and loops wait for the grid.
          </span>
        </div>
      );
    case "error":
      return (
        <div
          className={cn(box, "items-start border-solid border-danger text-left text-danger")}
          role="alert"
        >
          <span className="font-semibold">
            {state.playing ? "Couldn't draw this track's waveform." : "Couldn't load this track."}
          </span>
          <span className="line-clamp-2 text-2xs select-text">{state.message}</span>
          <span className="flex gap-1.5">
            <Button size="xs" onClick={onRetry}>
              Retry
            </Button>
            {!state.playing && (
              <Button size="xs" onClick={onEject}>
                Eject
              </Button>
            )}
            <ReportErrorButton message={state.message} />
          </span>
        </div>
      );
  }
}
