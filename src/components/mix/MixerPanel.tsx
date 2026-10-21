/**
 * The mixer as laid out on the hardware: four channel strips and the master
 * section, then filter X, the crossfader (with its curve) and filter Y.
 */
import type { FilterUnit as Unit } from "../../lib/ipc";
import { pickSwitches } from "../../lib/mixView";
import type { ChannelValues, FilterValues, MasterValues } from "../../store/mix";
import { Caption } from "../ui/Caption";
import { Fader } from "../ui/Fader";
import { ChannelLegend, ChannelStrip } from "./ChannelStrip";
import { FilterUnit } from "./FilterUnit";
import { LabelledKnob } from "./LabelledKnob";
import { MasterSection } from "./MasterSection";

export interface MixerPanelProps {
  channels: ChannelValues[];
  /** Which channels' decks have a track. */
  active: boolean[];
  filters: Record<Unit, FilterValues>;
  crossfader: { pos: number; curve: number };
  master: MasterValues;
  onChannel: <K extends keyof ChannelValues>(ch: number, param: K, value: ChannelValues[K]) => void;
  onFilter: <K extends keyof FilterValues>(unit: Unit, param: K, value: FilterValues[K]) => void;
  onCrossfader: (patch: Partial<{ pos: number; curve: number }>) => void;
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
  return (
    <section
      aria-label="Mixer"
      className="flex shrink-0 flex-col gap-2 rounded-md border border-border bg-surface p-2"
    >
      <div className="flex">
        <ChannelLegend />
        {p.channels.map((values, ch) => (
          <ChannelStrip
            // biome-ignore lint/suspicious/noArrayIndexKey: channels are fixed slots
            key={ch}
            ch={ch}
            values={values}
            active={p.active[ch] ?? false}
            onChange={(param, value) => p.onChannel(ch, param, value)}
            onCue={(additive) => cue(ch, additive)}
          />
        ))}
        <MasterSection values={p.master} onChange={p.onMaster} />
      </div>
      <div className="flex items-start gap-2 border-t border-border-subtle pt-2">
        <FilterUnit unit="x" values={p.filters.x} onChange={(k, v) => p.onFilter("x", k, v)} />
        <div className="flex min-w-0 flex-1 flex-col items-center gap-1.5 pt-5">
          <div className="flex w-full items-center gap-1">
            <Caption aria-hidden="true">X</Caption>
            <Fader
              orientation="horizontal"
              value={p.crossfader.pos}
              defaultValue={0.5}
              onValueChange={(pos) => p.onCrossfader({ pos })}
              aria-label="Crossfader"
            />
            <Caption aria-hidden="true">Y</Caption>
          </div>
          <LabelledKnob
            caption="Curve"
            size="sm"
            value={p.crossfader.curve}
            onValueChange={(curve) => p.onCrossfader({ curve })}
            aria-label="Crossfader curve"
          />
        </div>
        <FilterUnit unit="y" values={p.filters.y} onChange={(k, v) => p.onFilter("y", k, v)} />
      </div>
    </section>
  );
}
