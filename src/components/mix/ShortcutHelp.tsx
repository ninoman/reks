/** The Mix panel's keyboard shortcuts, in a popover (the keys are `lib/mixKeys`, rekordbox's layout). */
import { WAVE_ZOOM_KEY } from "../../lib/keys";
import { crossfaderKeyHint, deckKeysHelp } from "../../lib/mixKeys";
import { mixerParts } from "../../lib/mixView";
import { useMix } from "../../store/mix";
import { Button } from "../ui/Button";
import { Kbd } from "../ui/Kbd";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/Popover";

/** The deck keys come from `deckKeysHelp`, so the list and the tooltips can't disagree. */
const CROSSFADER_ROW: [string, string] = [crossfaderKeyHint(), "Crossfader left, centre, right"];

const SHORTCUTS: [string, string][] = [
  [deckKeysHelp([{ type: "cue" }]), "Cue deck 2 / 3 (hold on the cue point to preview)"],
  [deckKeysHelp([{ type: "play" }]), "Play or pause deck 2 / 3"],
  [deckKeysHelp([{ type: "sync" }]), "Sync"],
  [deckKeysHelp([{ type: "master" }]), "Take master"],
  [
    deckKeysHelp([
      { type: "nudge", dir: -1 },
      { type: "nudge", dir: 1 },
    ]),
    "Nudge slower, faster (hold)",
  ],
  [
    deckKeysHelp(
      [
        { type: "hotCue", slot: 0 },
        { type: "hotCue", slot: 3 },
      ],
      true,
    ),
    "Hot cues A–D (any pad mode)",
  ],
  [
    deckKeysHelp([
      { type: "callCue", dir: -1 },
      { type: "callCue", dir: 1 },
    ]),
    "Previous, next memory cue",
  ],
  [deckKeysHelp([{ type: "memoryCue" }]), "Set a memory cue at the playhead"],
  [
    deckKeysHelp([
      { type: "jump", dir: -1 },
      { type: "jump", dir: 1 },
    ]),
    "Beat jump back, forward by the last jump size",
  ],
  [deckKeysHelp([{ type: "loop" }]), "Loop on or off at the next loop size"],
  [
    deckKeysHelp([{ type: "loopIn" }, { type: "loopOut" }]),
    "Loop IN, OUT (an active loop's start, end)",
  ],
  [deckKeysHelp([{ type: "reloop" }]), "Reloop: back into the last loop, or leave this one"],
  [
    deckKeysHelp([
      { type: "loopResize", double: false },
      { type: "loopResize", double: true },
    ]),
    "Halve, double the loop (or the next loop size)",
  ],
  [deckKeysHelp([{ type: "padMode" }]), "Next pad mode"],
  ["⌥ + key", "Same on deck 1 / 4"],
  CROSSFADER_ROW,
  [
    `${WAVE_ZOOM_KEY.out} / ${WAVE_ZOOM_KEY.in}`,
    "Zoom every deck's waveform out, in (or ⌘/Ctrl + scroll on one)",
  ],
];

export function ShortcutHelp() {
  // The Simple mixer view (either EQ layout) has no crossfader, so its keys do nothing.
  const crossfader = useMix((s) => mixerParts(s.look).crossfader);
  const rows = crossfader ? SHORTCUTS : SHORTCUTS.filter((row) => row !== CROSSFADER_ROW);
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size="sm" variant="ghost">
          Shortcuts
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="p-2">
        <table className="text-xs">
          <tbody>
            {rows.map(([keys, what]) => (
              <tr key={keys}>
                <td className="py-0.5 pr-3">
                  <Kbd>{keys}</Kbd>
                </td>
                <td className="text-fg-muted">{what}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </PopoverContent>
    </Popover>
  );
}
