/**
 * Filter unit X or Y, from the mixer's edge inwards: the ON lamp (captioned with the unit's
 * letter), FREQ, RESONANCE and a column of HP / BP / LP lamps; `stacked`, in two rows. A click picks one mode;
 * Shift+click combines modes (`pickSwitches`). The unit on the `right` is the mirror image of
 * the one on the left, around the mixer's centre line.
 */
import { cn } from "../../lib/cn";
import type { FilterUnit as Unit } from "../../lib/ipc";
import { pickSwitches } from "../../lib/mixView";
import type { FilterValues } from "../../store/mix";
import { Caption } from "../ui/Caption";
import { LampButton } from "../ui/LampButton";
import { Tooltip } from "../ui/Tooltip";
import { LabelledKnob } from "./LabelledKnob";

export interface FilterUnitProps {
  unit: Unit;
  side: "left" | "right";
  values: FilterValues;
  onChange: <K extends keyof FilterValues>(param: K, value: FilterValues[K]) => void;
  /** Two columns, ON and FREQ over RES and the modes (the one-channel mixer's column). */
  stacked?: boolean;
}

type Mode = "highPass" | "bandPass" | "lowPass";

/** `[param, button text, spoken name]`. */
const MODES: [Mode, string, string][] = [
  ["highPass", "HP", "high-pass"],
  ["bandPass", "BP", "band-pass"],
  ["lowPass", "LP", "low-pass"],
];

export function FilterUnit({ unit, side, values, onChange, stacked = false }: FilterUnitProps) {
  const name = `Filter ${unit.toUpperCase()}`;
  const pickMode = (i: number, additive: boolean) => {
    const was = MODES.map(([p]) => values[p]);
    pickSwitches(was, i, additive, true).forEach((on, j) => {
      const mode = MODES[j]?.[0];
      if (mode !== undefined && on !== was[j]) onChange(mode, on);
    });
  };
  const mirror = side === "right" && "flex-row-reverse";
  return (
    <section
      aria-label={name}
      className={cn(
        stacked
          ? "grid grid-cols-2 items-end justify-items-center gap-x-2 gap-y-1"
          : "flex items-end gap-(--mixer-filter-gap)",
        mirror,
      )}
    >
      {/* mb-1: the knobs' caps sit ~4 px above their boxes' bottom edge; lamps line up with the caps. */}
      <div className="mb-1 flex flex-col items-center gap-1">
        <Caption tone="default" aria-hidden="true">
          {unit.toUpperCase()}
        </Caption>
        <LampButton
          pressed={values.on}
          aria-label={`${name} on`}
          onClick={() => onChange("on", !values.on)}
        />
      </div>
      <LabelledKnob
        caption="Freq"
        size="lg"
        defaultValue={0.5}
        value={values.freq}
        onValueChange={(v) => onChange("freq", v)}
        aria-label={`${name} frequency`}
      />
      <LabelledKnob
        caption="Res"
        size="sm"
        value={values.resonance}
        onValueChange={(v) => onChange("resonance", v)}
        aria-label={`${name} resonance`}
      />
      {/* HP, BP, LP top to bottom on both units; only the lamp/caption order mirrors. */}
      <div className={cn("mb-1 flex flex-col gap-0.5", side === "right" && "items-end")}>
        {MODES.map(([p, text, spoken], i) => (
          <div key={p} className={cn("flex items-center gap-1", mirror)}>
            <Tooltip content="Shift+click to combine modes">
              <LampButton
                pressed={values[p]}
                aria-label={`${name} ${spoken}`}
                onClick={(e) => pickMode(i, e.shiftKey)}
              />
            </Tooltip>
            <Caption tone={values[p] ? "default" : "muted"} aria-hidden="true">
              {text}
            </Caption>
          </div>
        ))}
      </div>
    </section>
  );
}
