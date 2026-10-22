/** The Mix panel's keyboard shortcuts, in a popover (the keys are `lib/mixKeys`). */
import { Button } from "../ui/Button";
import { Kbd } from "../ui/Kbd";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/Popover";

const SHORTCUTS: [string, string][] = [
  ["A / J", "Cue deck 2 / 3 (hold on the cue point to preview)"],
  ["S / K", "Play or pause deck 2 / 3"],
  ["D / L", "Sync deck 2 / 3"],
  ["Q W / U I", "Nudge slower, faster (hold)"],
  ["1–4 / 7–0", "Hot cues A–D (any pad mode)"],
  ["E R / O P", "Beat jump back, forward by the last jump size"],
  ["F / ;", "Loop on or off at the last loop size"],
  ["G / H", "Take master"],
  ["T / Y", "Next pad mode"],
  ["Alt A / Alt J", "Set a memory cue at the playhead"],
  ["Alt E R / Alt O P", "Previous, next memory cue"],
  ["Shift + key", "Same on deck 1 / 4"],
  ["Z X C", "Crossfader left, centre, right"],
  ["− / +", "Zoom every deck's waveform out, in (or ⌘/Ctrl + scroll on one)"],
];

export function ShortcutHelp() {
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
            {SHORTCUTS.map(([keys, what]) => (
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
