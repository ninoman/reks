/**
 * A deck's pads: the mode switch (HOT CUE, BEAT JUMP, BEAT LOOP), Q, the
 * page arrows and 2 × 4 pads. HOT CUE: A–H in their cue colours; an empty
 * (dashed) pad sets its cue at the playhead, a set one jumps to it,
 * double-click renames it in place, its × (hover or focus) or Shift+click
 * deletes it; the memory
 * cue controls take the page arrows' place. BEAT JUMP: split pads, back on the left, forward on
 * the right. BEAT LOOP: a loop of that size, the active size lit (press it
 * again to leave the loop). The size used last has a heavy outline: the
 * keys use it. Jumps, loops and Q wait for a grid.
 */
import { type CSSProperties, useState } from "react";

import { formatRgb, hotCueLabel } from "../../lib/format";
import type { CueEdit, HotCueView, MemoryCueView } from "../../lib/ipc";
import type { DeckAction } from "../../lib/mixKeys";
import {
  formatBeats,
  HOT_CUE_PADS,
  JUMP_PAGES,
  LOOP_PAGES,
  MAX_CUE_NAME_CHARS,
  type PadMode,
} from "../../lib/mixView";
import type { PagedPadMode } from "../../store/mix";
import { Button } from "../ui/Button";
import { InlineTextField } from "../ui/InlineText";
import { Pad, padVariants, SplitPad } from "../ui/Pad";
import { Segment, SegmentedControl } from "../ui/SegmentedControl";
import { ShortcutTooltip } from "../ui/Tooltip";
import { DeckMemory } from "./DeckMemory";
import { PagePicker } from "./PagePicker";

/** `prefix` drops on a narrow deck (pads under 20rem) so the labels and the memory controls fit on one line. */
const MODES: { mode: PadMode; prefix: string; label: string }[] = [
  { mode: "hotCue", prefix: "HOT", label: "CUE" },
  { mode: "beatJump", prefix: "BEAT", label: "JUMP" },
  { mode: "beatLoop", prefix: "BEAT", label: "LOOP" },
];

const PAGES: Record<PagedPadMode, readonly (readonly number[])[]> = {
  beatJump: JUMP_PAGES,
  beatLoop: LOOP_PAGES,
};

const beatsWord = (b: number) =>
  b === 1 / 32 ? "FINE (1/32 beat)" : `${formatBeats(b)} beat${b === 1 ? "" : "s"}`;

export interface DeckPadsProps {
  deck: number;
  loaded: boolean;
  /** The engine has a grid for the track. */
  grid: boolean;
  mode: PadMode;
  page: Record<PagedPadMode, number>;
  hotCues: HotCueView[];
  /** By position. */
  memoryCues: MemoryCueView[];
  positionMs: number;
  /** Beats of the active loop, null when not looping. */
  loopBeats: number | null;
  quantise: boolean;
  lastJump: number;
  lastLoop: number;
  keyFor: (action: DeckAction) => string | undefined;
  onMode: (mode: PadMode) => void;
  onPage: (mode: PagedPadMode, page: number) => void;
  onQuantise: () => void;
  /** Jumps to a set hot cue, sets an empty one. */
  onHotCuePad: (slot: number) => void;
  onEditCues: (edit: CueEdit) => void;
  /** Cue point and playhead to a memory cue. */
  onCallCue: (ms: number) => void;
  onCallAdjacent: (dir: -1 | 1) => void;
  onJump: (beats: number) => void;
  onLoop: (beats: number) => void;
}

export function DeckPads(props: DeckPadsProps) {
  const { deck, loaded, grid, mode, page, quantise, keyFor, onMode, onPage, onQuantise } = props;
  const n = deck + 1;
  const paged = mode === "hotCue" ? null : mode;
  return (
    <div className="@container flex min-w-0 max-w-(--deck-pads-max-w) flex-1 flex-col gap-1">
      <div className="flex h-(--ctl-h-xs) shrink-0 items-center gap-1">
        <ShortcutTooltip shortcut={keyFor({ type: "padMode" })} hint="Next pad mode">
          <SegmentedControl
            size="xs"
            variant="control"
            aria-label={`Deck ${n} pad mode`}
            value={mode}
            onValueChange={(v) => onMode(v as PadMode)}
            disabled={!loaded}
          >
            {MODES.map((m) => (
              <Segment
                key={m.mode}
                value={m.mode}
                aria-label={`${m.prefix} ${m.label}`}
                className="gap-1 px-1 text-2xs tracking-tight whitespace-nowrap"
              >
                <span className="hidden @[20rem]:inline">{m.prefix}</span>
                {m.label}
              </Segment>
            ))}
          </SegmentedControl>
        </ShortcutTooltip>
        <Button
          size="xs"
          variant="control"
          disabled={!loaded || !grid}
          aria-pressed={quantise}
          aria-label={`Deck ${n} quantise`}
          className="w-(--ctl-h-xs) px-0"
          onClick={onQuantise}
        >
          Q
        </Button>
        <span className="flex-1" />
        {paged === null ? (
          <DeckMemory
            deck={deck}
            loaded={loaded}
            cues={props.memoryCues}
            positionMs={props.positionMs}
            looping={props.loopBeats !== null}
            setKey={keyFor({ type: "memoryCue" })}
            prevKey={keyFor({ type: "callCue", dir: -1 })}
            nextKey={keyFor({ type: "callCue", dir: 1 })}
            onSet={() => props.onEditCues({ type: "addMemoryCue" })}
            onCall={props.onCallCue}
            onAdjacent={props.onCallAdjacent}
            onRename={(cueId, name) => props.onEditCues({ type: "rename", cueId, name })}
            onAutoLoop={(cueId, on) => props.onEditCues({ type: "setActiveLoop", cueId, on })}
            onDelete={(cueId) => props.onEditCues({ type: "delete", cueId })}
          />
        ) : (
          <PagePicker
            page={page[paged]}
            pages={PAGES[paged].length}
            disabled={!loaded || !grid}
            label={`Deck ${n} ${paged === "beatLoop" ? "loop" : "jump"} sizes`}
            onPage={(p) => onPage(paged, p)}
          />
        )}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-4 grid-rows-2 gap-1">
        {mode === "hotCue" ? (
          <HotCuePads {...props} />
        ) : mode === "beatJump" ? (
          <JumpPads {...props} sizes={JUMP_PAGES[page.beatJump] ?? []} />
        ) : (
          <LoopPads {...props} sizes={LOOP_PAGES[page.beatLoop] ?? []} />
        )}
      </div>
    </div>
  );
}

function HotCuePads({ deck, loaded, hotCues, keyFor, onHotCuePad, onEditCues }: DeckPadsProps) {
  const n = deck + 1;
  const [renaming, setRenaming] = useState<number | null>(null);
  return Array.from({ length: HOT_CUE_PADS }, (_, slot) => {
    const cue = loaded ? hotCues.find((c) => c.slot === slot) : undefined;
    const letter = hotCueLabel(slot);
    const color = cue?.colorRgb == null ? undefined : formatRgb(cue.colorRgb);
    if (cue !== undefined && renaming === slot) {
      return (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: hot cue slots are fixed
          key={slot}
          className={padVariants({ tone: "set" })}
          style={color === undefined ? undefined : ({ "--pad-color": color } as CSSProperties)}
        >
          <b>{letter}</b>
          <InlineTextField
            initial={cue.name ?? ""}
            maxLength={MAX_CUE_NAME_CHARS}
            aria-label={`Deck ${n} hot cue ${letter} name`}
            className="h-full text-2xs text-fg"
            onDone={(commit, name) => {
              setRenaming(null);
              if (commit && name !== cue.name) onEditCues({ type: "rename", cueId: cue.id, name });
            }}
          />
        </div>
      );
    }
    return (
      <Pad
        // biome-ignore lint/suspicious/noArrayIndexKey: hot cue slots are fixed
        key={slot}
        tone={cue !== undefined ? "set" : "empty"}
        color={color}
        disabled={!loaded}
        aria-label={`Deck ${n} hot cue ${letter}`}
        hint={
          cue === undefined
            ? `Set hot cue ${letter} here`
            : `Hot cue ${letter}${cue.name ? `: ${cue.name}` : ""} · double-click to rename · × or Shift+click to delete`
        }
        shortcut={keyFor({ type: "hotCue", slot })}
        onClick={(e) => {
          if (cue !== undefined && e.shiftKey) onEditCues({ type: "delete", cueId: cue.id });
          else onHotCuePad(slot);
        }}
        onDoubleClick={() => cue !== undefined && setRenaming(slot)}
        remove={
          cue === undefined
            ? undefined
            : {
                label: `Delete deck ${n} hot cue ${letter}`,
                onRemove: () => onEditCues({ type: "delete", cueId: cue.id }),
              }
        }
      >
        <b>{letter}</b>
        {cue?.name != null && (
          <span className="min-w-0 truncate font-medium opacity-70">{cue.name}</span>
        )}
      </Pad>
    );
  });
}

function JumpPads({
  deck,
  loaded,
  grid,
  lastJump,
  keyFor,
  onJump,
  sizes,
}: DeckPadsProps & { sizes: readonly number[] }) {
  const n = deck + 1;
  return sizes.map((b) => {
    const marked = b === lastJump;
    return (
      <SplitPad
        key={b}
        label={formatBeats(b)}
        marked={marked}
        disabled={!loaded || !grid}
        backLabel={`Deck ${n} jump back ${beatsWord(b)}`}
        forwardLabel={`Deck ${n} jump forward ${beatsWord(b)}`}
        backShortcut={marked ? keyFor({ type: "jump", dir: -1 }) : undefined}
        forwardShortcut={marked ? keyFor({ type: "jump", dir: 1 }) : undefined}
        onBack={() => onJump(-b)}
        onForward={() => onJump(b)}
      />
    );
  });
}

function LoopPads({
  deck,
  loaded,
  grid,
  loopBeats,
  lastLoop,
  keyFor,
  onLoop,
  sizes,
}: DeckPadsProps & { sizes: readonly number[] }) {
  const n = deck + 1;
  return sizes.map((b) => {
    const active = loopBeats === b;
    return (
      <Pad
        key={b}
        tone={active ? "lit" : "default"}
        marked={b === lastLoop}
        disabled={!loaded || !grid}
        aria-pressed={active}
        aria-label={`Deck ${n} loop ${beatsWord(b)}`}
        hint={active ? "Leave the loop" : `Loop ${beatsWord(b)}`}
        shortcut={b === lastLoop ? keyFor({ type: "loop" }) : undefined}
        className="font-mono"
        onClick={() => onLoop(b)}
      >
        {formatBeats(b)}
      </Pad>
    );
  });
}
