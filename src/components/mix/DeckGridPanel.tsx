/**
 * A deck's grid mode controls (D10, phase 5 task 30), in place of its pads,
 * loop block and SYNC: a row with the playhead's bar.beat and its offset
 * from the beat, and the BPM of the section there (− / +, double-click to
 * type) and a menu; then six pads, three over three: PIN (a thin red line
 * and 1, "beat 1 here"; 2 and a thin white line, stretch to here: the
 * two-point fit), SHIFT and ½ BPM ×2, then MARKER, HISTORY (undo / redo) and LOCK (lit
 * when locked: the other pads are off, and analysis or an XML re-import
 * keeps the grid). SHIFT and − / + step once per press and
 * repeat while held (bigger steps); Shift-click takes the big step. Edits
 * that place a position snap to transients unless Alt is held. Every pad's
 * tooltip names its key; the keys work while the deck has focus.
 */
import type { MouseEvent, ReactNode } from "react";
import { formatBpm } from "../../lib/format";
import { GRID_KEYS, type HoldPhase } from "../../lib/gridMode";
import { LockIcon, MinusIcon, MoreIcon, PlusIcon, RedoIcon, UndoIcon } from "../icons";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../ui/DropdownMenu";
import { IconButton } from "../ui/IconButton";
import { InlineNumber } from "../ui/InlineNumber";
import { Pad, SplitPad } from "../ui/Pad";
import { useHoldRepeat } from "./useHoldRepeat";

export type DeckGridState = "loading" | "noGrid" | "locked" | "ready";

export interface DeckGridPanelProps {
  deck: number;
  state: DeckGridState;
  /** Bar.beat at the playhead ("18.1") and its offset ("+6 ms"); null without beats. */
  position: string | null;
  offset: string | null;
  /** Tempo of the section at the playhead. */
  bpmX100: number | null;
  /** The playhead is on a marker: the marker pad removes it. */
  onMarker: boolean;
  /** A grid edited in this session or before: Reset can bring the old one back. */
  manual: boolean;
  canUndo: boolean;
  canRedo: boolean;
  /** Tempo from the taps so far (no grid yet). */
  tapBpmX100: number | null;
  busy: boolean;
  error: string | null;
  actions: DeckGridActions;
}

export interface DeckGridActions {
  shift: (dir: -1 | 1, phase: HoldPhase, big: boolean) => void;
  bpmStep: (dir: -1 | 1, phase: HoldPhase, big: boolean) => void;
  setBpm: (bpm: number) => void;
  /** `exact`: Alt held, no snap. */
  beatOne: (exact: boolean) => void;
  stretch: (exact: boolean) => void;
  marker: (exact: boolean) => void;
  halve: () => void;
  double: () => void;
  undo: () => void;
  redo: () => void;
  tap: () => void;
  /** Starts a grid at the tapped tempo with beat 1 at the playhead. */
  useTap: () => void;
  reset: () => void;
  setLocked: (locked: boolean) => void;
  exit: () => void;
}

const exact = (e: MouseEvent) => e.altKey;

/** A grid point as a thin line, like a beat line. */
function PinMark({ className }: { className: string }) {
  return <span aria-hidden="true" className={`h-3 w-0.5 rounded-full ${className}`} />;
}

export function DeckGridPanel({
  deck,
  state,
  position,
  offset,
  bpmX100,
  onMarker,
  manual,
  canUndo,
  canRedo,
  tapBpmX100,
  busy,
  error,
  actions: a,
}: DeckGridPanelProps) {
  const n = deck + 1;
  const ready = state === "ready";
  const shiftBack = useHoldRepeat((phase, big) => a.shift(-1, phase, big));
  const shiftForward = useHoldRepeat((phase, big) => a.shift(1, phase, big));
  const bpmDown = useHoldRepeat((phase, big) => a.bpmStep(-1, phase, big));
  const bpmUp = useHoldRepeat((phase, big) => a.bpmStep(1, phase, big));

  const header = (
    <div className="flex min-w-0 items-center gap-1.5 font-mono text-xs tabular-nums">
      {error !== null ? (
        <span role="alert" title={error} className="min-w-0 flex-1 truncate text-danger">
          {error}
        </span>
      ) : (
        <>
          <span className="font-bold">{position ?? "–"}</span>
          <span className="min-w-0 flex-1 truncate text-fg-muted">
            {state === "loading" ? "Loading grid…" : (offset ?? "")}
          </span>
        </>
      )}
      <IconButton
        size="xs"
        variant="control"
        aria-label={`Deck ${n} grid BPM down`}
        hint="BPM −0.01 (held: 0.10 steps), the pin stays"
        shortcut={GRID_KEYS.bpmDown}
        disabled={!ready}
        {...bpmDown}
      >
        <MinusIcon />
      </IconButton>
      <InlineNumber
        value={state === "loading" ? null : bpmX100 === null ? null : bpmX100 / 100}
        format={(v) => v.toFixed(2)}
        onCommit={a.setBpm}
        disabled={!ready}
        aria-label={`Deck ${n} grid BPM`}
        title="Grid BPM of this section: double-click to type"
        className="w-14 text-center text-sm font-bold"
      />
      <IconButton
        size="xs"
        variant="control"
        aria-label={`Deck ${n} grid BPM up`}
        hint="BPM +0.01 (held: 0.10 steps), the pin stays"
        shortcut={GRID_KEYS.bpmUp}
        disabled={!ready}
        {...bpmUp}
      >
        <PlusIcon />
      </IconButton>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <IconButton size="xs" variant="ghost" aria-label={`Deck ${n} grid menu`}>
            <MoreIcon />
          </IconButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem disabled={!manual || state === "locked"} onSelect={a.reset}>
            Reset grid
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={a.exit}>Leave grid mode</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );

  let pads: ReactNode;
  if (state === "noGrid") {
    pads = (
      <>
        <Pad aria-label={`Deck ${n} tap tempo`} hint="Tap along with the beat" onClick={a.tap}>
          TAP
        </Pad>
        <Pad
          aria-label={`Deck ${n} start a grid here`}
          hint="Start a grid at the tapped BPM with beat 1 at the playhead"
          disabled={tapBpmX100 === null || busy}
          onClick={a.useTap}
        >
          {tapBpmX100 === null ? "Grid here" : `${formatBpm(tapBpmX100)} here`}
        </Pad>
        <div className="row-span-2 flex items-center text-2xs text-fg-muted">
          No grid yet: tap along, then start one at the playhead.
        </div>
      </>
    );
  } else {
    const locked = state === "locked";
    pads = (
      <>
        <SplitPad
          label="PIN"
          backGlyph={
            <span className="flex items-center gap-1">
              <PinMark className="bg-grid-pin-1" />1
            </span>
          }
          forwardGlyph={
            <span className="flex items-center gap-1">
              2<PinMark className="bg-grid-pin-2" />
            </span>
          }
          backLabel="Pin 1: the nearest beat onto the playhead as beat 1, pinned (Alt: exact)"
          forwardLabel="Pin 2: the beat nearest the playhead onto it; pin 1 stays (Alt: exact)"
          backShortcut={GRID_KEYS.beatOne}
          forwardShortcut={GRID_KEYS.stretch}
          backProps={{ onClick: (e) => a.beatOne(exact(e)) }}
          forwardProps={{ onClick: (e) => a.stretch(exact(e)) }}
          disabled={!ready}
        />
        <SplitPad
          label="SHIFT"
          backLabel="Shift grid earlier: 1 ms, held 10 ms steps"
          forwardLabel="Shift grid later: 1 ms, held 10 ms steps"
          backShortcut={GRID_KEYS.shiftBack}
          forwardShortcut={GRID_KEYS.shiftForward}
          backProps={shiftBack}
          forwardProps={shiftForward}
          disabled={!ready}
        />
        <SplitPad
          label="BPM"
          backGlyph="½"
          forwardGlyph="×2"
          backLabel="Halve this section's BPM"
          forwardLabel="Double this section's BPM"
          backShortcut={GRID_KEYS.halve}
          forwardShortcut={GRID_KEYS.double}
          onBack={a.halve}
          onForward={a.double}
          disabled={!ready}
        />
        <Pad
          aria-label={onMarker ? `Deck ${n} remove marker` : `Deck ${n} add marker`}
          hint={
            onMarker
              ? "Remove this tempo section marker"
              : "New tempo section from the playhead (Alt: exact)"
          }
          shortcut={GRID_KEYS.marker}
          disabled={!ready}
          onClick={(e) => a.marker(exact(e))}
        >
          {onMarker ? "− Marker" : "+ Marker"}
        </Pad>
        <SplitPad
          label="HISTORY"
          backGlyph={<UndoIcon />}
          forwardGlyph={<RedoIcon />}
          backLabel="Undo"
          forwardLabel="Redo"
          backShortcut={GRID_KEYS.undo}
          forwardShortcut={GRID_KEYS.redo}
          onBack={a.undo}
          onForward={a.redo}
          disabled={locked || (!canUndo && !canRedo)}
          backProps={{ disabled: locked || !canUndo }}
          forwardProps={{ disabled: locked || !canRedo }}
        />
        <Pad
          aria-label={`Deck ${n} lock grid`}
          aria-pressed={locked}
          tone={locked ? "lit" : "default"}
          hint={
            locked
              ? "Unlock the grid to edit it"
              : "Lock the grid: no edits, and analysis or an XML re-import keeps it"
          }
          disabled={state === "loading"}
          onClick={() => a.setLocked(!locked)}
        >
          <LockIcon open={!locked} />
          LOCK
        </Pad>
      </>
    );
  }

  return (
    <section
      aria-label={`Deck ${n} grid`}
      aria-busy={busy || undefined}
      className="grid h-full min-h-0 min-w-0 grid-rows-(--deck-ctl-rows) gap-y-1"
    >
      {header}
      <div className="row-span-2 grid min-h-0 grid-cols-3 grid-rows-2 gap-1">{pads}</div>
    </section>
  );
}
