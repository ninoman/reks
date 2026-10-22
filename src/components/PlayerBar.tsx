/**
 * The deck section's toolbar, under the app header and above the player deck
 * or the Mix panel (as in rekordbox and Traktor, decks sit above the
 * library): the deck layout (1 deck is the player deck alone, 2 or 4 open
 * the Mix panel), the output's status line (device and rate, off, opening,
 * or why it is not playing) with Start / Stop output, the mix shortcuts
 * with the Mix panel open, then volume and output device. How the output
 * opens (rate, buffer, headphone cue) is in Settings → Playback; loading a
 * track opens it too.
 */
import { useEffect } from "react";

import { outputSummary } from "../lib/mixView";
import { useMix } from "../store/mix";
import { usePanels } from "../store/panels";
import { chooseOutput } from "../store/settings";
import { ShortcutHelp } from "./mix/ShortcutHelp";
import { Button } from "./ui/Button";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Select } from "./ui/Select";
import { Slider } from "./ui/Slider";

export function PlayerBar() {
  const mixOpen = usePanels((s) => s.mixOpen);
  const deckCount = useMix((s) => s.deckCount);
  const volume = useMix((s) => s.volume);
  const device = useMix((s) => s.config.device);
  const devices = useMix((s) => s.devices);
  const error = useMix((s) => s.error);
  const status = useMix((s) => s.status);
  // A string, so the 60 Hz engine state re-renders the bar only when it changes.
  const output = useMix((s) =>
    s.status === "running" && s.live !== null ? outputSummary(s.live) : null,
  );

  useEffect(() => {
    void useMix.getState().loadDevices();
  }, []);

  // A chosen device that is not connected stays listed so the choice shows.
  const options = device !== null && !devices.includes(device) ? [device, ...devices] : devices;

  function setLayout(v: string) {
    usePanels.getState().setMixOpen(v !== "1");
    if (v === "2" || v === "4") useMix.getState().setDeckCount(v === "4" ? 4 : 2);
  }

  return (
    <section
      className="flex h-(--player-bar-h) flex-none items-center gap-2.5 px-3"
      aria-label="Player"
    >
      <SegmentedControl
        size="sm"
        aria-label="Decks shown"
        value={mixOpen ? String(deckCount) : "1"}
        onValueChange={setLayout}
      >
        <Segment value="1">1 deck</Segment>
        <Segment value="2">2 decks</Segment>
        <Segment value="4">4 decks</Segment>
      </SegmentedControl>
      <span className="min-w-0 flex-1 truncate text-xs">
        {error !== null ? (
          <span role="alert" className="text-danger select-text">
            {error}
          </span>
        ) : (
          <span role="status" className="text-fg-muted">
            {status === "starting" ? "Opening the audio output…" : (output ?? "Audio output off")}
          </span>
        )}
      </span>
      {mixOpen && <ShortcutHelp />}
      {output !== null ? (
        <Button size="sm" className="flex-none" onClick={() => void useMix.getState().stop()}>
          Stop output
        </Button>
      ) : (
        <Button
          size="sm"
          className="flex-none"
          disabled={status === "starting"}
          onClick={() => void useMix.getState().start()}
        >
          Start output
        </Button>
      )}
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
        onChange={(e) =>
          void chooseOutput({ device: e.target.value === "" ? null : e.target.value })
        }
      >
        <option value="">System default</option>
        {options.map((d) => (
          <option key={d} value={d}>
            {d}
          </option>
        ))}
      </Select>
    </section>
  );
}
