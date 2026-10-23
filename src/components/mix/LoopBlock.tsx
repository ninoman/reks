/**
 * The quick-loop block, on every deck in every pad mode: IN, OUT, ½, the
 * size, 2×, LOOP and RELOOP. `row` is one line (under the overview), `grid`
 * the 3 × 2 block beside the pads (IN OUT / ½ size 2× / LOOP RELOOP). What
 * can't act yet is disabled, not hidden. IN, OUT and RELOOP need no grid;
 * LOOP and ½ / 2× of the next size do.
 */
import { cva } from "class-variance-authority";

import { cn } from "../../lib/cn";
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
  layout: "row" | "grid";
  state: LoopBlockState;
  loaded: boolean;
  keyFor: (action: DeckAction) => string | undefined;
  actions: LoopBlockActions;
}

const block = cva("min-w-0 shrink-0", {
  variants: {
    layout: {
      row: "flex h-6 items-center gap-1",
      grid: "grid w-(--loop-block-w) grid-cols-[1.5rem_1fr_1fr_1.5rem] grid-rows-3 gap-1",
    },
  },
});

/** Button cells: the grid spans them over its four columns. */
const span = { row: "", grid: "col-span-2" } as const;

const text = "px-1 text-2xs font-bold tracking-caption";

export function LoopBlock({ deck, layout, state, loaded, keyFor, actions }: LoopBlockProps) {
  const n = deck + 1;
  const size = layout === "row" ? "sm" : "xs";
  const wide = span[layout];
  const sizeWord = state.looping ? "loop size" : "next loop size";
  return (
    <fieldset aria-label={`Deck ${n} quick loop`} className={block({ layout })}>
      <Button
        size={size}
        variant="control"
        disabled={!loaded}
        aria-pressed={state.inLit}
        aria-label={`Deck ${n} loop in`}
        hint={state.looping ? "Move the loop start here" : "Loop in at the playhead"}
        shortcut={keyFor({ type: "loopIn" })}
        className={cn(text, wide)}
        onClick={actions.loopIn}
      >
        IN
      </Button>
      <Button
        size={size}
        variant="control"
        disabled={!state.outEnabled}
        aria-label={`Deck ${n} loop out`}
        hint={state.looping ? "Move the loop end here" : "Loop out: loop from IN to here"}
        shortcut={keyFor({ type: "loopOut" })}
        className={cn(text, wide)}
        onClick={actions.loopOut}
      >
        OUT
      </Button>
      <Button
        size={size}
        variant="control"
        disabled={!state.resizeEnabled}
        aria-label={`Deck ${n} halve loop`}
        hint={state.looping ? "Halve the loop" : "Halve the next loop size"}
        shortcut={keyFor({ type: "loopResize", double: false })}
        className={cn(text, layout === "grid" ? "col-start-1 px-0" : "ml-1")}
        onClick={() => actions.loopResize(false)}
      >
        ½
      </Button>
      <Readout
        size={size}
        dimmed={state.size.dimmed}
        aria-label={`Deck ${n} ${sizeWord}: ${state.size.text}`}
        className={cn(layout === "grid" ? "col-span-2" : "min-w-11")}
      >
        {state.size.text}
      </Readout>
      <Button
        size={size}
        variant="control"
        disabled={!state.resizeEnabled}
        aria-label={`Deck ${n} double loop`}
        hint={state.looping ? "Double the loop" : "Double the next loop size"}
        shortcut={keyFor({ type: "loopResize", double: true })}
        className={cn(text, layout === "grid" && "px-0")}
        onClick={() => actions.loopResize(true)}
      >
        2×
      </Button>
      <Button
        size={size}
        variant="control"
        disabled={!state.loopEnabled}
        aria-pressed={state.looping}
        aria-label={`Deck ${n} loop`}
        hint={state.looping ? "Leave the loop" : "Loop at the shown size"}
        shortcut={keyFor({ type: "loop" })}
        className={cn(text, wide, layout === "row" && "ml-1")}
        onClick={actions.toggleLoop}
      >
        LOOP
      </Button>
      <Button
        size={size}
        variant="control"
        disabled={!state.reloopEnabled}
        aria-label={`Deck ${n} reloop`}
        hint={state.looping ? "Leave the loop, keeping it" : "Back into the last loop"}
        shortcut={keyFor({ type: "reloop" })}
        className={cn(text, wide)}
        onClick={actions.reloop}
      >
        RELOOP
      </Button>
    </fieldset>
  );
}
