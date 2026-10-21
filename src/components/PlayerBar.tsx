/**
 * The player's header row, above the player deck (or the Mix panel when it
 * is on): volume, output device, the Mix toggle, and why the output is not
 * playing (a failed start, a lost device) while the Mix panel is off. The deck itself is `PlayerDeck`.
 */
import { useEffect } from "react";

import { useMix } from "../store/mix";
import { usePanels } from "../store/panels";
import { chooseOutputDevice } from "../store/settings";
import { Button } from "./ui/Button";
import { Select } from "./ui/Select";
import { Slider } from "./ui/Slider";

export function PlayerBar() {
  const mixOpen = usePanels((s) => s.mixOpen);
  const volume = useMix((s) => s.volume);
  const device = useMix((s) => s.config.device);
  const devices = useMix((s) => s.devices);
  const error = useMix((s) => s.error);
  const starting = useMix((s) => s.status === "starting");

  useEffect(() => {
    void useMix.getState().loadDevices();
  }, []);

  // A chosen device that is not connected stays listed so the choice shows.
  const options = device !== null && !devices.includes(device) ? [device, ...devices] : devices;

  return (
    <section
      className="flex h-(--player-bar-h) flex-none items-center gap-2.5 border-t border-border bg-bg-subtle px-3"
      aria-label="Player"
    >
      <span className="min-w-0 flex-1 truncate text-xs" role="status">
        {/* The Mix panel shows its own errors. */}
        {mixOpen ? null : error !== null ? (
          <span className="text-danger select-text">{error}</span>
        ) : starting ? (
          <span className="text-fg-muted">Opening the audio output…</span>
        ) : null}
      </span>
      <Slider
        className="w-22.5 flex-none"
        min={0}
        max={1}
        step={0.01}
        value={volume}
        aria-label="Volume"
        onValueChange={(v) => useMix.getState().setVolume(v)}
      />
      <Select
        size="sm"
        className="max-w-40 flex-none"
        aria-label="Output device"
        value={device ?? ""}
        onFocus={() => void useMix.getState().loadDevices()}
        onChange={(e) => void chooseOutputDevice(e.target.value === "" ? null : e.target.value)}
      >
        <option value="">System default</option>
        {options.map((d) => (
          <option key={d} value={d}>
            {d}
          </option>
        ))}
      </Select>
      <Button
        size="sm"
        className="flex-none"
        aria-pressed={mixOpen}
        onClick={() => usePanels.getState().toggleMix()}
      >
        Mix
      </Button>
    </section>
  );
}
