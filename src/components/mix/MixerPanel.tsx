/**
 * The mixer as laid out on the hardware: a strip per shown deck (four, or
 * channels 2 and 3 with two decks; the hidden ones keep their positions) and the master
 * section (3-band: the FX select and PARAMETER in the legend), then filter X
 * and Y (4-band), and the crossfader across the mixer with its curve at the
 * right end. Spare height (4 decks) spreads the strips' rows.
 */
import type { FilterUnit as Unit } from "../../lib/ipc";
import { crossfaderKeyHint } from "../../lib/mixKeys";
import { type MixerLook, mixerParts, pickSwitches } from "../../lib/mixView";
import type { ChannelValues, FilterValues, FxValues, MasterValues } from "../../store/mix";
import { useSettings } from "../../store/settings";
import { GearIcon } from "../icons";
import { Caption } from "../ui/Caption";
import { Fader } from "../ui/Fader";
import { IconButton } from "../ui/IconButton";
import { Knob } from "../ui/Knob";
import { Tooltip } from "../ui/Tooltip";
import { ChannelLegend, ChannelStrip } from "./ChannelStrip";
import { FilterUnit } from "./FilterUnit";
import { MasterSection } from "./MasterSection";

export interface MixerPanelProps {
  channels: ChannelValues[];
  /** The channels drawn, by index: the shown decks'. */
  shown: readonly number[];
  /** Which channels' decks have a track. */
  active: boolean[];
  filters: Record<Unit, FilterValues>;
  crossfader: { pos: number; curve: number };
  fx: FxValues;
  master: MasterValues;
  look: MixerLook;
  onChannel: <K extends keyof ChannelValues>(ch: number, param: K, value: ChannelValues[K]) => void;
  onFilter: <K extends keyof FilterValues>(unit: Unit, param: K, value: FilterValues[K]) => void;
  onCrossfader: (patch: Partial<{ pos: number; curve: number }>) => void;
  onFx: (patch: Partial<FxValues>) => void;
  onMaster: <K extends keyof MasterValues>(param: K, value: MasterValues[K]) => void;
}

export function MixerPanel(p: MixerPanelProps) {
  // A click cues one channel alone; Shift+click adds or removes it.
  const cue = (ch: number, additive: boolean) => {
    const was = p.channels.map((c) => c.cue);
    pickSwitches(was, ch, additive, false).forEach((on, i) => {
      if (on !== was[i]) p.onChannel(i, "cue", on);
    });
  };
  const parts = mixerParts(p.look);
  return (
    <section
      aria-label="Mixer"
      className="relative flex h-(--mixer-h) w-(--mixer-w) shrink-0 flex-col gap-1 rounded-md border border-border bg-surface p-2"
    >
      <div className="flex min-h-0 flex-1 justify-center">
        <IconButton
          size="xs"
          className="absolute top-1 left-1"
          aria-label="Mixer settings"
          onClick={() => useSettings.getState().setDialogOpen(true, "mixer")}
        >
          <GearIcon size={14} />
        </IconButton>
        <ChannelLegend parts={parts} fx={{ values: p.fx, onChange: p.onFx }} />
        {p.shown.map((ch) => {
          const values = p.channels[ch];
          if (values === undefined) return null;
          return (
            <ChannelStrip
              key={ch}
              ch={ch}
              values={values}
              active={p.active[ch] ?? false}
              parts={parts}
              onChange={(param, value) => p.onChannel(ch, param, value)}
              onCue={(additive) => cue(ch, additive)}
            />
          );
        })}
        <MasterSection values={p.master} compact={!parts.filterAssigns} onChange={p.onMaster} />
      </div>
      {parts.filterRow && (
        <div className="flex justify-between border-t border-border-subtle pt-1">
          <FilterUnit
            unit="x"
            side="left"
            values={p.filters.x}
            onChange={(k, v) => p.onFilter("x", k, v)}
          />
          <FilterUnit
            unit="y"
            side="right"
            values={p.filters.y}
            onChange={(k, v) => p.onFilter("y", k, v)}
          />
        </div>
      )}
      {parts.crossfader && (
        <div className="flex h-(--mixer-row-h) items-center gap-1">
          <Caption aria-hidden="true">X</Caption>
          <Tooltip content="Crossfader left, centre, right" shortcut={crossfaderKeyHint()}>
            <Fader
              orientation="horizontal"
              value={p.crossfader.pos}
              defaultValue={0.5}
              onValueChange={(pos) => p.onCrossfader({ pos })}
              aria-label="Crossfader"
            />
          </Tooltip>
          <Caption aria-hidden="true">Y</Caption>
          <Caption aria-hidden="true" className="ml-1">
            Curve
          </Caption>
          <Knob
            size="sm"
            value={p.crossfader.curve}
            onValueChange={(curve) => p.onCrossfader({ curve })}
            aria-label="Crossfader curve"
          />
        </div>
      )}
    </section>
  );
}
