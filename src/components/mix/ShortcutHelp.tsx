/** The Mix panel's keyboard shortcuts, in a popover (the keys are `lib/mixKeys`, rekordbox's layout). */
import { Button } from "../ui/Button";
import { Kbd } from "../ui/Kbd";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/Popover";

const SHORTCUTS: [string, string][] = [
  ["A / H", "Cue deck 2 / 3 (hold on the cue point to preview)"],
  ["Z / N", "Play or pause deck 2 / 3"],
  ["Q / Y", "Sync"],
  ["Shift Q / Shift Y", "Take master"],
  ["W E / U I", "Nudge slower, faster (hold)"],
  ["1–4 / 6–9", "Hot cues A–D (any pad mode)"],
  ["R T / O P", "Previous, next memory cue"],
  ["Shift T / Shift P", "Set a memory cue at the playhead"],
  ["⌘ R T / ⌘ O P", "Beat jump back, forward by the last jump size"],
  ["Shift F / Shift L", "Loop on or off at the last loop size"],
  ["Shift S / Shift J", "Next pad mode"],
  ["⌥ + key", "Same on deck 1 / 4"],
  ["X C V", "Crossfader left, centre, right"],
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
