/**
 * The quick-loop block, on every deck in every pad mode, laid out like
 * rekordbox's: BEAT | IN/OUT tabs level with the pad mode tabs, then one
 * page over the two pad rows (`--deck-ctl-rows`), as wide as
 * `--loop-block-w`. BEAT: the size is the loop button (lit while any loop
 * plays), ‹ › halve and double it. IN/OUT: IN and OUT, then RELOOP, which
 * reads EXIT while looping. The hidden page's tab has a dot while a loop
 * plays or IN waits. What can't act yet is disabled, not hidden. IN, OUT and
 * RELOOP need no grid; the size and ‹ › of the next size do.
 */
import type { DeckAction } from "../../lib/mixKeys";
import type { LoopBlockState, LoopPage } from "../../lib/mixView";
import { Button } from "../ui/Button";
import { Segment, SegmentedControl } from "../ui/SegmentedControl";

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
  page: LoopPage;
  onPage: (page: LoopPage) => void;
  keyFor: (action: DeckAction) => string | undefined;
  actions: LoopBlockActions;
}

const PAGES: readonly { page: LoopPage; label: string; name: string }[] = [
  { page: "beat", label: "BEAT", name: "Beat loop" },
  { page: "manual", label: "IN/OUT", name: "In and out" },
];

/** Every button fills its cell; the wide ones span both columns. */
const cell = "h-full px-0 text-2xs font-bold tracking-caption";
const wide = `${cell} col-span-2`;

export function LoopBlock({ deck, state, loaded, page, onPage, keyFor, actions }: LoopBlockProps) {
  const n = deck + 1;
  return (
    <fieldset
      aria-label={`Deck ${n} quick loop`}
      className="grid w-(--loop-block-w) min-w-0 shrink-0 grid-cols-2 grid-rows-(--deck-ctl-rows) gap-1"
    >
      <SegmentedControl
        size="xs"
        variant="outline"
        aria-label={`Deck ${n} quick loop page`}
        value={page}
        onValueChange={(v) => onPage(v as LoopPage)}
        disabled={!loaded}
        className="col-span-2 min-w-0"
      >
        {PAGES.map((p) => {
          const dot = state.pageDot && p.page !== page;
          return (
            <Segment
              key={p.page}
              value={p.page}
              aria-label={dot ? `${p.name}, active` : p.name}
              className="min-w-0 flex-1 justify-center gap-1 px-1 text-2xs font-semibold"
            >
              {p.label}
              {dot && <span aria-hidden className="size-1 shrink-0 rounded-full bg-current" />}
            </Segment>
          );
        })}
      </SegmentedControl>
      {page === "beat" ? (
        <>
          <Button
            size="xs"
            variant="control"
            disabled={!state.loopEnabled}
            aria-pressed={state.looping}
            aria-label={`Deck ${n} loop`}
            hint={state.looping ? "Leave the loop" : `Loop ${state.sizeText}`}
            shortcut={keyFor({ type: "loop" })}
            className="col-span-2 h-full px-0 font-mono text-lg tabular-nums"
            onClick={actions.toggleLoop}
          >
            {state.sizeText}
          </Button>
          <Button
            size="xs"
            variant="control"
            disabled={!state.resizeEnabled}
            aria-label={`Deck ${n} halve loop`}
            hint={state.looping ? "Halve the loop" : "Halve the next loop size"}
            shortcut={keyFor({ type: "loopResize", double: false })}
            className={`${cell} text-md`}
            onClick={() => actions.loopResize(false)}
          >
            ‹
          </Button>
          <Button
            size="xs"
            variant="control"
            disabled={!state.resizeEnabled}
            aria-label={`Deck ${n} double loop`}
            hint={state.looping ? "Double the loop" : "Double the next loop size"}
            shortcut={keyFor({ type: "loopResize", double: true })}
            className={`${cell} text-md`}
            onClick={() => actions.loopResize(true)}
          >
            ›
          </Button>
        </>
      ) : (
        <>
          <Button
            size="xs"
            variant="control"
            disabled={!loaded}
            aria-pressed={state.inLit}
            aria-label={`Deck ${n} loop in`}
            hint={state.looping ? "Move the loop start here" : "Loop in at the playhead"}
            shortcut={keyFor({ type: "loopIn" })}
            className={cell}
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
            className={cell}
            onClick={actions.loopOut}
          >
            OUT
          </Button>
          <Button
            size="xs"
            variant="control"
            disabled={!state.reloopEnabled}
            aria-pressed={state.looping}
            aria-label={`Deck ${n} ${state.looping ? "exit loop" : "reloop"}`}
            hint={state.looping ? "Leave the loop, keeping it" : "Back into the last loop"}
            shortcut={keyFor({ type: "reloop" })}
            className={wide}
            onClick={actions.reloop}
          >
            {state.looping ? "EXIT" : "RELOOP"}
          </Button>
        </>
      )}
    </fieldset>
  );
}
