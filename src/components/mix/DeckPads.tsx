/**
 * A deck's pads: the mode switch (HOT CUE, BEAT JUMP, BEAT LOOP), Q, the
 * page arrows and 2 × 4 pads. HOT CUE: A–H in their cue colours (empty ones
 * dashed and disabled). BEAT JUMP: split pads, back on the left, forward on
 * the right. BEAT LOOP: a loop of that size, the active size lit (press it
 * again to leave the loop). The size used last has a heavy outline: the
 * keys use it. Jumps, loops and Q wait for a grid.
 */
import { formatRgb, hotCueLabel } from "../../lib/format";
import type { HotCueView } from "../../lib/ipc";
import type { DeckAction } from "../../lib/mixKeys";
import { formatBeats, HOT_CUE_PADS, JUMP_PAGES, LOOP_PAGES, type PadMode } from "../../lib/mixView";
import type { PagedPadMode } from "../../store/mix";
import { Button } from "../ui/Button";
import { Pad, SplitPad } from "../ui/Pad";
import { Segment, SegmentedControl } from "../ui/SegmentedControl";
import { ShortcutTooltip } from "../ui/Tooltip";
import { PagePicker } from "./PagePicker";

/** `prefix` drops on a narrow deck (a 1280 px window) so the labels fit on one line. */
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
  /** Beats of the active loop, null when not looping. */
  loopBeats: number | null;
  quantise: boolean;
  lastJump: number;
  lastLoop: number;
  keyFor: (action: DeckAction) => string | undefined;
  onMode: (mode: PadMode) => void;
  onPage: (mode: PagedPadMode, page: number) => void;
  onQuantise: () => void;
  onHotCue: (slot: number) => void;
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
                className="gap-1 px-1.5 text-2xs tracking-tight whitespace-nowrap"
              >
                <span className="hidden @[18rem]:inline">{m.prefix}</span>
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
        <PagePicker
          page={paged === null ? 0 : page[paged]}
          pages={paged === null ? 1 : PAGES[paged].length}
          disabled={paged === null || !loaded || !grid}
          label={`Deck ${n} ${paged === "beatLoop" ? "loop" : "jump"} sizes`}
          onPage={(p) => paged !== null && onPage(paged, p)}
        />
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

function HotCuePads({ deck, loaded, hotCues, keyFor, onHotCue }: DeckPadsProps) {
  const n = deck + 1;
  return Array.from({ length: HOT_CUE_PADS }, (_, slot) => {
    const cue = hotCues.find((c) => c.slot === slot);
    const letter = hotCueLabel(slot);
    const set = loaded && cue !== undefined;
    return (
      <Pad
        // biome-ignore lint/suspicious/noArrayIndexKey: hot cue slots are fixed
        key={slot}
        tone={set ? "set" : "empty"}
        color={cue?.colorRgb == null ? undefined : formatRgb(cue.colorRgb)}
        disabled={!set}
        aria-label={`Deck ${n} hot cue ${letter}`}
        hint={`Hot cue ${letter}${cue?.name ? `: ${cue.name}` : ""}`}
        shortcut={keyFor({ type: "hotCue", slot })}
        onClick={() => onHotCue(slot)}
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
