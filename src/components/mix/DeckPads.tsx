/**
 * A deck's pads: the mode tabs (HOT CUE, MEMORY, JUMP, LOOP), then the
 * page arrows (JUMP and LOOP: sizes; MEMORY: cues),
 * and 2 × 4 pads. HOT CUE: A–H outlined in their cue colours; an empty
 * (dashed) pad sets its cue at the playhead, a set one jumps to it,
 * double-click renames it in place, its × (hover or focus) or Shift+click
 * deletes it. MEMORY: the first eight memory cues by position, the same
 * way (time, LOOP / AUTO for a memory loop, name); a set pad calls the cue,
 * an empty one sets one at the playhead, and right-click has
 * jump, rename, AUTO (memory loops) and delete. BEAT JUMP: split pads, back on the
 * left, forward on the right. BEAT LOOP: a loop of that size, the active
 * size lit (press it again to leave the loop). The size used last has a
 * heavy outline: the keys use it. Jumps and loops wait for a grid.
 */
import { type CSSProperties, useState } from "react";

import { formatRgb, hotCueLabel } from "../../lib/format";
import type { CueEdit, HotCueView, MemoryCueView } from "../../lib/ipc";
import type { DeckAction } from "../../lib/mixKeys";
import {
  formatBeats,
  formatClock,
  HOT_CUE_PADS,
  JUMP_PAGES,
  LOOP_PAGES,
  MAX_CUE_NAME_CHARS,
  MEMORY_PADS,
  type PadMode,
} from "../../lib/mixView";
import type { PagedPadMode } from "../../store/mix";
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "../ui/ContextMenu";
import { InlineTextField } from "../ui/InlineText";
import { Pad, padVariants, SplitPad } from "../ui/Pad";
import { Segment, SegmentedControl } from "../ui/SegmentedControl";
import { ShortcutTooltip } from "../ui/Tooltip";
import { PagePicker } from "./PagePicker";

/** `prefix` / `suffix` drop on a narrow deck (pads under 20rem) so the labels and the memory controls fit on one line. */
const MODES: { mode: PadMode; prefix: string; label: string; suffix: string }[] = [
  { mode: "hotCue", prefix: "HOT", label: "CUE", suffix: "" },
  { mode: "memory", prefix: "", label: "MEM", suffix: "ORY" },
  { mode: "beatJump", prefix: "", label: "JUMP", suffix: "" },
  { mode: "beatLoop", prefix: "", label: "LOOP", suffix: "" },
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
  /** Beats of the active loop, null when not looping. */
  loopBeats: number | null;
  lastJump: number;
  lastLoop: number;
  keyFor: (action: DeckAction) => string | undefined;
  onMode: (mode: PadMode) => void;
  onPage: (mode: PagedPadMode, page: number) => void;
  /** Jumps to a set hot cue, sets an empty one. */
  onHotCuePad: (slot: number) => void;
  onEditCues: (edit: CueEdit) => void;
  /** Cue point and playhead to a memory cue. */
  onCallCue: (ms: number) => void;
  onJump: (beats: number) => void;
  onLoop: (beats: number) => void;
}

export function DeckPads(props: DeckPadsProps) {
  const { deck, loaded, grid, mode, page, keyFor, onMode, onPage } = props;
  const n = deck + 1;
  const paged = mode === "hotCue" || mode === "memory" ? null : mode;
  // Memory cue pages follow the track's cues, so they are not kept per deck like sizes.
  const memPages = Math.max(1, Math.ceil(props.memoryCues.length / MEMORY_PADS));
  const [memPageWanted, setMemPage] = useState(0);
  const memPage = Math.min(memPageWanted, memPages - 1);
  return (
    <div className="@container flex min-w-0 max-w-(--deck-pads-max-w) flex-1 flex-col gap-1">
      <div className="flex h-(--ctl-h-xs) shrink-0 items-center gap-1">
        {/* Mode tabs, not a lit switch (that is Q's look): equal tabs, the chosen one outlined like the hot cues. */}
        <ShortcutTooltip shortcut={keyFor({ type: "padMode" })} hint="Next pad mode">
          <SegmentedControl
            size="xs"
            variant="outline"
            aria-label={`Deck ${n} pad mode`}
            value={mode}
            onValueChange={(v) => onMode(v as PadMode)}
            disabled={!loaded}
            className="min-w-0 flex-1"
          >
            {MODES.map((m) => (
              <Segment
                key={m.mode}
                value={m.mode}
                aria-label={m.prefix === "" ? m.label + m.suffix : `${m.prefix} ${m.label}`}
                className="min-w-0 flex-1 justify-center gap-1 px-1 text-2xs font-semibold whitespace-nowrap"
              >
                {m.prefix !== "" && <span className="hidden @[20rem]:inline">{m.prefix}</span>}
                <span>
                  {m.label}
                  {m.suffix !== "" && <span className="hidden @[20rem]:inline">{m.suffix}</span>}
                </span>
              </Segment>
            ))}
          </SegmentedControl>
        </ShortcutTooltip>
        {mode === "memory" ? (
          <PagePicker
            page={memPage}
            pages={memPages}
            disabled={!loaded}
            label={`Deck ${n} memory cues`}
            onPage={setMemPage}
          />
        ) : paged === null ? (
          // Hot cues have one page; the picker stays (disabled) so the row doesn't shift.
          <PagePicker
            page={0}
            pages={1}
            disabled={!loaded}
            label={`Deck ${n} hot cues`}
            onPage={() => {}}
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
        ) : mode === "memory" ? (
          <MemoryPads {...props} page={memPage} />
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

function MemoryPads({
  deck,
  loaded,
  memoryCues,
  page,
  onCallCue,
  onEditCues,
}: Omit<DeckPadsProps, "page"> & { page: number }) {
  const n = deck + 1;
  const [renaming, setRenaming] = useState<number | null>(null);
  const cues = loaded ? memoryCues.slice(page * MEMORY_PADS, (page + 1) * MEMORY_PADS) : [];
  return Array.from({ length: MEMORY_PADS }, (_, i) => {
    const cue = cues[i];
    if (cue === undefined) {
      return (
        <Pad
          // biome-ignore lint/suspicious/noArrayIndexKey: pad positions are fixed
          key={i}
          tone="empty"
          disabled={!loaded}
          aria-label={`Deck ${n} set memory cue (pad ${i + 1})`}
          hint="Set a memory cue here"
          onClick={() => onEditCues({ type: "addMemoryCue" })}
        />
      );
    }
    const at = formatClock(cue.positionMs);
    const what = cue.endMs === null ? "cue" : "loop";
    const style = {
      "--pad-color": cue.colorRgb == null ? "var(--color-deck-memory)" : formatRgb(cue.colorRgb),
    } as CSSProperties;
    const label = (
      <>
        <b className="font-mono tabular-nums">{at}</b>
        {cue.endMs !== null && (
          <span className="opacity-70">{cue.activeLoop ? "AUTO" : "LOOP"}</span>
        )}
      </>
    );
    if (renaming === cue.id) {
      return (
        <div key={cue.id} className={padVariants({ tone: "set" })} style={style}>
          {label}
          <InlineTextField
            initial={cue.name ?? ""}
            maxLength={MAX_CUE_NAME_CHARS}
            aria-label={`Deck ${n} memory ${what} at ${at} name`}
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
      <ContextMenu key={cue.id}>
        <ContextMenuTrigger asChild>
          {/* The menu needs one element: a Pad with its × is two buttons. */}
          <div className="flex min-w-0 *:min-w-0 *:flex-1">
            <Pad
              tone="set"
              style={style}
              aria-label={`Deck ${n} memory ${what} at ${at}`}
              hint={`Memory ${what} ${at}${cue.name ? `: ${cue.name}` : ""} · double-click to rename · × or Shift+click to delete · right-click for more`}
              onClick={(e) => {
                if (e.shiftKey) onEditCues({ type: "delete", cueId: cue.id });
                else onCallCue(cue.positionMs);
              }}
              onDoubleClick={() => setRenaming(cue.id)}
              remove={{
                label: `Delete deck ${n} memory ${what} at ${at}`,
                onRemove: () => onEditCues({ type: "delete", cueId: cue.id }),
              }}
            >
              {label}
              {cue.name != null && (
                <span className="min-w-0 truncate font-medium opacity-70">{cue.name}</span>
              )}
            </Pad>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onSelect={() => onCallCue(cue.positionMs)}>Jump here</ContextMenuItem>
          <ContextMenuItem onSelect={() => setRenaming(cue.id)}>Rename</ContextMenuItem>
          {cue.endMs !== null && (
            <ContextMenuCheckboxItem
              checked={cue.activeLoop}
              onCheckedChange={(on) => onEditCues({ type: "setActiveLoop", cueId: cue.id, on })}
            >
              Auto loop: playback enters it by itself
            </ContextMenuCheckboxItem>
          )}
          <ContextMenuSeparator />
          <ContextMenuItem
            variant="danger"
            onSelect={() => onEditCues({ type: "delete", cueId: cue.id })}
          >
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
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
