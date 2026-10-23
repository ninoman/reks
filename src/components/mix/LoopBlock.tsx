/**
 * The quick-loop block, on every deck in every pad mode: IN, OUT, ½, the
 * size, 2×, LOOP and RELOOP: a 3-row block right of the pads, its rows
 * level with theirs (`--deck-ctl-rows`: IN OUT by the mode tabs, ½ size 2×
 * and LOOP RELOOP by the pad rows), as wide as `--loop-block-w`. What
 * can't act yet is disabled, not hidden. IN, OUT and RELOOP need no grid;
 * LOOP and ½ / 2× of the next size do.
 */
import type { DeckAction } from "../../lib/mixKeys";
import type { LoopBlockState } from "../../lib/mixView";
import { Button } from "../ui/Button";
import { Readout } from "../ui/Readout";

export interface LoopBlockActions {
  loopIn: () => void;
  loopOut: () => void;
  loopResize: (double: boolean) => void;
  toggleLoop: () => void;
  reloop: () => void;
}

export interface LoopBlockProps {
  deck: number;
  state: LoopBlockState;
  loaded: boolean;
  keyFor: (action: DeckAction) => string | undefined;
  actions: LoopBlockActions;
}

/** Every button fills its cell: IN, OUT, LOOP and RELOOP span two of the four columns. */
const cell = "h-full px-0 text-2xs font-bold tracking-caption";
const wide = `${cell} col-span-2`;

export function LoopBlock({ deck, state, loaded, keyFor, actions }: LoopBlockProps) {
  const n = deck + 1;
  const sizeWord = state.looping ? "loop size" : "next loop size";
  return (
    <fieldset
      aria-label={`Deck ${n} quick loop`}
      className="grid w-(--loop-block-w) min-w-0 shrink-0 grid-cols-4 grid-rows-(--deck-ctl-rows) gap-1"
    >
      <Button
        size="xs"
        variant="control"
        disabled={!loaded}
        aria-pressed={state.inLit}
        aria-label={`Deck ${n} loop in`}
        hint={state.looping ? "Move the loop start here" : "Loop in at the playhead"}
        shortcut={keyFor({ type: "loopIn" })}
        className={wide}
        onClick={actions.loopIn}
      >
        IN
      </Button>
      <Button
        size="xs"
        variant="control"
        disabled={!state.outEnabled}
        aria-label={`Deck ${n} loop out`}
        hint={state.looping ? "Move the loop end here" : "Loop out: loop from IN to here"}
        shortcut={keyFor({ type: "loopOut" })}
        className={wide}
        onClick={actions.loopOut}
      >
        OUT
      </Button>
      <Button
        size="xs"
        variant="control"
        disabled={!state.resizeEnabled}
        aria-label={`Deck ${n} halve loop`}
        hint={state.looping ? "Halve the loop" : "Halve the next loop size"}
        shortcut={keyFor({ type: "loopResize", double: false })}
        className={cell}
        onClick={() => actions.loopResize(false)}
      >
        ½
      </Button>
      <Readout
        size="xs"
        dimmed={state.size.dimmed}
        aria-label={`Deck ${n} ${sizeWord}: ${state.size.text}`}
        className="col-span-2 h-full"
      >
        {state.size.text}
      </Readout>
      <Button
        size="xs"
        variant="control"
        disabled={!state.resizeEnabled}
        aria-label={`Deck ${n} double loop`}
        hint={state.looping ? "Double the loop" : "Double the next loop size"}
        shortcut={keyFor({ type: "loopResize", double: true })}
        className={cell}
        onClick={() => actions.loopResize(true)}
      >
        2×
      </Button>
      <Button
        size="xs"
        variant="control"
        disabled={!state.loopEnabled}
        aria-pressed={state.looping}
        aria-label={`Deck ${n} loop`}
        hint={state.looping ? "Leave the loop" : "Loop at the shown size"}
        shortcut={keyFor({ type: "loop" })}
        className={wide}
        onClick={actions.toggleLoop}
      >
        LOOP
      </Button>
      <Button
        size="xs"
        variant="control"
        disabled={!state.reloopEnabled}
        aria-label={`Deck ${n} reloop`}
        hint={state.looping ? "Leave the loop, keeping it" : "Back into the last loop"}
        shortcut={keyFor({ type: "reloop" })}
        className={wide}
        onClick={actions.reloop}
      >
        RELOOP
      </Button>
    </fieldset>
  );
}
