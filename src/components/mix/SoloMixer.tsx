/**
 * The one-channel mixer beside the player deck (1 deck, task 31): the
 * legend and channel 2's LEVEL and EQ knobs, then a column on the right with
 * the channel's filter unit (4-band) or its FX (3-band). No fader, master or
 * cue: the deck-header chip says when the fader or master level is off its
 * default. As tall as the player deck; the rows share its height.
 */
import type { FilterUnit as Unit } from "../../lib/ipc";
import { defaultFilterAssign, knobRows, type MixerLook, mixerParts } from "../../lib/mixView";
import type { ChannelValues, FilterValues, FxValues } from "../../store/mix";
import { useSettings } from "../../store/settings";
import { GearIcon } from "../icons";
import { Caption } from "../ui/Caption";
import { IconButton } from "../ui/IconButton";
import { AssignSwitch } from "./AssignSwitch";
import { ChannelKnob } from "./ChannelStrip";
import { FilterUnit } from "./FilterUnit";
import { FxParameter, FxSelect } from "./FxControls";

export interface SoloMixerProps {
  /** The player deck's channel, by index. */
  ch: number;
  values: ChannelValues;
  filters: Record<Unit, FilterValues>;
  fx: FxValues;
  look: MixerLook;
  onChannel: <K extends keyof ChannelValues>(ch: number, param: K, value: ChannelValues[K]) => void;
  onFilter: <K extends keyof FilterValues>(unit: Unit, param: K, value: FilterValues[K]) => void;
  onFx: (patch: Partial<FxValues>) => void;
}

export function SoloMixer(p: SoloMixerProps) {
  const parts = mixerParts(p.look);
  // The strip ends at the EQ: in 3-band the FX knob moves to the column.
  const rows = knobRows(parts).filter(([param]) => param !== "fx");
  const column = parts.threeBand || parts.filterRow;
  return (
    <section
      aria-label="Mixer"
      className={
        rows.length === 4
          ? "grid h-(--deck-solo-h) shrink-0 grid-rows-[repeat(4,minmax(var(--mixer-row-h),1fr))] items-center gap-x-1 rounded-md border border-border bg-surface p-2"
          : "grid h-(--deck-solo-h) shrink-0 grid-rows-[repeat(5,minmax(var(--mixer-row-h),1fr))] items-center gap-x-1 rounded-md border border-border bg-surface p-2"
      }
    >
      {column && (
        <div className="col-start-3 row-span-full row-start-1 ml-1 flex h-full w-(--solo-mixer-column-w) flex-col items-center justify-evenly border-l border-border-subtle pl-2">
          {parts.threeBand ? <FxColumn {...p} /> : <FilterColumn {...p} />}
        </div>
      )}
      {rows.map(([param, caption], i) => [
        <div key={`${param}-caption`} className="col-start-1 flex items-center justify-end gap-2">
          {i === 0 && (
            <IconButton
              size="xs"
              className="mr-auto"
              aria-label="Mixer settings"
              onClick={() => useSettings.getState().setDialogOpen(true, "mixer")}
            >
              <GearIcon size={14} />
            </IconButton>
          )}
          <Caption aria-hidden="true">{caption}</Caption>
        </div>,
        <div key={param} className="col-start-2 flex justify-center">
          <ChannelKnob
            ch={p.ch}
            param={param}
            caption={caption}
            values={p.values}
            onChange={(k, v) => p.onChannel(p.ch, k, v)}
          />
        </div>,
      ])}
    </section>
  );
}

/** 4-band: FILTER, the assign in Advanced (Simple uses the default unit), then that unit. */
function FilterColumn(p: SoloMixerProps) {
  const parts = mixerParts(p.look);
  const unit = parts.filterAssigns ? p.values.filterAssign : defaultFilterAssign(p.ch);
  return (
    <>
      <Caption tone="default" aria-hidden="true">
        Filter
      </Caption>
      {parts.filterAssigns && (
        <AssignSwitch
          label={`Channel ${p.ch + 1} filter assign`}
          value={p.values.filterAssign}
          onChange={(v) => p.onChannel(p.ch, "filterAssign", v)}
        />
      )}
      {unit === "off" ? (
        <Caption>Off</Caption>
      ) : (
        <FilterUnit
          unit={unit}
          side="left"
          stacked
          values={p.filters[unit]}
          onChange={(k, v) => p.onFilter(unit, k, v)}
        />
      )}
    </>
  );
}

/** 3-band: the FX select, then the channel's FX knob over PARAMETER. */
function FxColumn(p: SoloMixerProps) {
  const fx = { values: p.fx, onChange: p.onFx };
  return (
    <>
      <FxSelect {...fx} />
      <div className="flex flex-col items-center gap-1">
        <Caption aria-hidden="true">Amount</Caption>
        <ChannelKnob
          ch={p.ch}
          param="fx"
          caption="FX"
          values={p.values}
          onChange={(k, v) => p.onChannel(p.ch, k, v)}
        />
      </div>
      <FxParameter {...fx} className="items-center gap-1" />
    </>
  );
}
