/**
 * The mix-prep panel (phase 5): decks on either side of the mixer, 2 by
 * default or 4. Connects the mixer store to the presentational deck and
 * mixer components. Before the output is started it shows the start form;
 * an error (start failed, device lost) shows above it.
 */
import { memo, useEffect, useMemo } from "react";

import { phaseOffset } from "../../lib/mixView";
import { type MixStore, useMix } from "../../store/mix";
import { Button } from "../ui/Button";
import { Kbd } from "../ui/Kbd";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/Popover";
import { Segment, SegmentedControl } from "../ui/SegmentedControl";
import { type DeckActions, DeckStrip } from "./DeckStrip";
import { MixerPanel } from "./MixerPanel";
import { MixStartForm } from "./MixStartForm";

/** The mixer does not depend on the 60 Hz engine state: skip those renders. */
const Mixer = memo(MixerPanel);

const SHORTCUTS: [string, string][] = [
  ["A / J", "Cue deck 1 / 2"],
  ["S / K", "Play or pause deck 1 / 2"],
  ["D / L", "Sync deck 1 / 2"],
  ["Q W / U I", "Nudge slower, faster (hold)"],
  ["1–4 / 7–0", "Hot cues A–D"],
  ["Shift + key", "Same on deck 3 / 4"],
  ["Z X C", "Crossfader left, centre, right"],
];

function deckActions(mix: MixStore, deck: number): DeckActions {
  return {
    togglePlay: () => mix.togglePlay(deck),
    cue: () => mix.send({ type: "cue", deck }),
    toggleSync: () => mix.toggleSync(deck),
    toggleBar: () => mix.toggleSync(deck, !(mix.live?.decks[deck]?.syncBar ?? false)),
    toggleLeader: () => {
      const chosen = mix.live?.leaderChosen === true && mix.live.leader === deck;
      mix.send({ type: "leader", deck: chosen ? null : deck });
    },
    toggleQuantise: () =>
      mix.send({ type: "quantise", deck, on: !(mix.live?.decks[deck]?.quantise ?? false) }),
    hotCue: (slot) => mix.send({ type: "hotCue", deck, slot }),
    nudge: (dir) => mix.nudge(deck, dir),
    setTempo: (ratio) => mix.setTempo(deck, ratio),
    setTempoRange: (range) => mix.setTempoRange(deck, range),
    seek: (ms) => mix.send({ type: "seek", deck, ms }),
    eject: () => mix.unload(deck),
    drop: (id) => void mix.load(deck, id),
  };
}

function ShortcutHelp() {
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

export function MixPanel() {
  const mix = useMix();
  const { status, live, deckCount } = mix;

  useEffect(() => {
    void useMix.getState().loadDevices();
  }, []);

  const running = status === "running" && live !== null;
  const shown = Array.from({ length: deckCount }, (_, i) => i);
  const compact = deckCount === 4;
  const activeKey = mix.decks.map((d) => (d.track === null ? 0 : 1)).join("");
  // Recomputed only when a deck loads or empties.
  const active = useMemo(() => [...activeKey].map((c) => c === "1"), [activeKey]);

  function strip(deck: number) {
    const view = live?.decks[deck];
    const meta = mix.decks[deck];
    if (meta === undefined) return null;
    const leader = live?.leader === deck;
    const leaderBeat = live?.leader == null ? null : (live.decks[live.leader]?.beat ?? null);
    const phase =
      !leader && view?.beat != null && leaderBeat !== null
        ? phaseOffset(view.beat, leaderBeat, view.syncBar ? 4 : 1)
        : null;
    return (
      <DeckStrip
        key={deck}
        deck={deck}
        view={view}
        meta={meta}
        leader={leader}
        leaderChosen={live?.leaderChosen ?? false}
        phase={phase}
        compact={compact}
        actions={deckActions(mix, deck)}
      />
    );
  }

  return (
    <section
      aria-label="Mix"
      className="flex max-h-(--mix-panel-max-h) shrink-0 flex-col gap-2 overflow-y-auto border-t border-border bg-bg-subtle px-3 pt-1.5 pb-2"
    >
      <div className="flex h-(--ctl-h-sm) items-center gap-2">
        <h2 className="text-sm font-semibold">Mix</h2>
        <span className="min-w-0 flex-1 truncate text-xs text-fg-muted" role="status">
          {running
            ? `${live.device} · ${(live.sampleRate / 1000).toFixed(1)} kHz${
                live.bufferFrames === null ? "" : ` · ${live.bufferFrames} frames`
              }${live.xruns > 0 ? ` · ${live.xruns} dropouts` : ""}`
            : status === "starting"
              ? "Opening the audio output…"
              : "Audio output off"}
        </span>
        <SegmentedControl
          size="sm"
          aria-label="Decks shown"
          value={String(deckCount)}
          onValueChange={(v) => mix.setDeckCount(v === "4" ? 4 : 2)}
        >
          <Segment value="2">2 decks</Segment>
          <Segment value="4">4 decks</Segment>
        </SegmentedControl>
        <ShortcutHelp />
        {running && (
          <Button size="sm" onClick={() => void mix.stop()}>
            Stop output
          </Button>
        )}
      </div>

      {mix.error !== null && (
        <p role="alert" className="text-xs text-danger select-text">
          {mix.error}
        </p>
      )}

      {!running ? (
        <div className="rounded-md border border-border bg-surface p-3">
          <p className="mb-3 text-sm text-fg-muted">
            Start the audio output to load tracks on decks and try a transition. Loading a track
            starts it too.
          </p>
          <MixStartForm
            config={mix.config}
            devices={mix.devices}
            options={mix.options}
            starting={status === "starting"}
            onChange={(patch) => void mix.setConfig(patch)}
            onStart={() => void mix.start()}
          />
        </div>
      ) : (
        <div className="flex min-h-0 items-stretch gap-2">
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            {shown.filter((d) => d % 2 === 0).map(strip)}
          </div>
          <Mixer
            channels={mix.channels}
            active={active}
            filters={mix.filters}
            crossfader={mix.crossfader}
            master={mix.master}
            onChannel={mix.setChannel}
            onFilter={mix.setFilter}
            onCrossfader={mix.setCrossfader}
            onMaster={mix.setMaster}
          />
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            {shown.filter((d) => d % 2 === 1).map(strip)}
          </div>
        </div>
      )}
    </section>
  );
}
