/**
 * A deck's memory cues (task 17): MEM sets one at the playhead (on the beat
 * with quantise), or saves the playing loop as a memory loop; ◀ ▶ call the
 * one before / after the playhead, and the count between them opens the
 * list, where a cue is jumped to, renamed (double-click its name) or
 * deleted (×), and a memory loop's AUTO makes playback enter it by itself.
 * Sits where the pad page arrows are in HOT CUE mode, which has no pages.
 */
import { formatPosition } from "../../lib/format";
import type { MemoryCueView } from "../../lib/ipc";
import { adjacentCue, MAX_CUE_NAME_CHARS } from "../../lib/mixView";
import { ChevronRightIcon, CloseIcon } from "../icons";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";
import { InlineText } from "../ui/InlineText";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/Popover";

export interface DeckMemoryProps {
  deck: number;
  loaded: boolean;
  /** By position. */
  cues: MemoryCueView[];
  positionMs: number;
  /** A loop is playing: MEM saves it. */
  looping: boolean;
  setKey: string | undefined;
  prevKey: string | undefined;
  nextKey: string | undefined;
  onSet: () => void;
  onCall: (ms: number) => void;
  onAdjacent: (dir: -1 | 1) => void;
  onRename: (cueId: number, name: string | null) => void;
  /** A memory loop playback enters by itself, or not. */
  onAutoLoop: (cueId: number, on: boolean) => void;
  onDelete: (cueId: number) => void;
}

export function DeckMemory({
  deck,
  loaded,
  cues,
  positionMs,
  looping,
  setKey,
  prevKey,
  nextKey,
  onSet,
  onCall,
  onAdjacent,
  onRename,
  onAutoLoop,
  onDelete,
}: DeckMemoryProps) {
  const n = deck + 1;
  const none = (dir: -1 | 1) => !loaded || adjacentCue(cues, positionMs, dir) === null;
  return (
    <div className="flex shrink-0 items-center">
      <Button
        size="xs"
        variant="control"
        disabled={!loaded}
        aria-label={`Deck ${n} set memory cue`}
        hint={looping ? "Save the loop as a memory loop" : "Set a memory cue here"}
        shortcut={setKey}
        className="mr-0.5 px-1 text-2xs font-bold tracking-caption"
        onClick={onSet}
      >
        MEM
      </Button>
      <IconButton
        size="xs"
        variant="ghost"
        aria-label={`Deck ${n} previous memory cue`}
        hint="Previous memory cue"
        shortcut={prevKey}
        disabled={none(-1)}
        onClick={() => onAdjacent(-1)}
        className="w-4 [&_svg]:rotate-180"
      >
        <ChevronRightIcon />
      </IconButton>
      <Popover>
        <PopoverTrigger asChild>
          <Button
            size="xs"
            variant="ghost"
            disabled={!loaded}
            aria-label={`Deck ${n} memory cues (${cues.length})`}
            className="min-w-4 px-0.5 font-mono text-2xs tabular-nums"
          >
            {loaded ? cues.length : "–"}
          </Button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-72 p-1">
          <MemoryList
            n={n}
            cues={cues}
            onCall={onCall}
            onRename={onRename}
            onAutoLoop={onAutoLoop}
            onDelete={onDelete}
          />
        </PopoverContent>
      </Popover>
      <IconButton
        size="xs"
        variant="ghost"
        aria-label={`Deck ${n} next memory cue`}
        hint="Next memory cue"
        shortcut={nextKey}
        disabled={none(1)}
        onClick={() => onAdjacent(1)}
        className="w-4"
      >
        <ChevronRightIcon />
      </IconButton>
    </div>
  );
}

function MemoryList({
  n,
  cues,
  onCall,
  onRename,
  onAutoLoop,
  onDelete,
}: Pick<DeckMemoryProps, "cues" | "onCall" | "onRename" | "onAutoLoop" | "onDelete"> & {
  n: number;
}) {
  if (cues.length === 0) {
    return (
      <p className="px-1.5 py-1 text-xs text-fg-muted">
        No memory cues. MEM sets one at the playhead, or saves the playing loop.
      </p>
    );
  }
  return (
    <ul aria-label={`Deck ${n} memory cues`} className="flex max-h-60 flex-col overflow-y-auto">
      {cues.map((c) => {
        const at = formatPosition(c.positionMs);
        const what = c.endMs === null ? "memory cue" : "memory loop";
        return (
          <li key={c.id} className="flex items-center gap-1 text-xs">
            <Button
              size="xs"
              variant="ghost"
              aria-label={`Jump to ${what} at ${at}`}
              title={c.endMs === null ? undefined : `Loop ${at} – ${formatPosition(c.endMs)}`}
              className="shrink-0 font-mono tabular-nums"
              onClick={() => onCall(c.positionMs)}
            >
              {at}
            </Button>
            <InlineText
              value={c.name}
              placeholder="No name"
              maxLength={MAX_CUE_NAME_CHARS}
              aria-label={`Name of ${what} at ${at}`}
              title="Double-click to rename"
              onCommit={(name) => onRename(c.id, name)}
            />
            {c.endMs !== null && (
              <Button
                size="xs"
                variant="control"
                aria-pressed={c.activeLoop}
                aria-label={`Auto loop at ${at}`}
                title="Auto loop: playback enters this loop by itself when it reaches the start"
                className="shrink-0 px-1 text-2xs font-bold tracking-caption"
                onClick={() => onAutoLoop(c.id, !c.activeLoop)}
              >
                AUTO
              </Button>
            )}
            <IconButton
              size="xs"
              variant="ghost"
              aria-label={`Delete ${what} at ${at}`}
              className="shrink-0"
              onClick={() => onDelete(c.id)}
            >
              <CloseIcon />
            </IconButton>
          </li>
        );
      })}
    </ul>
  );
}
