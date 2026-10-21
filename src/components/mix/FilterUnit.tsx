/**
 * Filter unit X or Y: FREQ, RESONANCE, HP / BP / LP, ON, VCF to crossfader,
 * and on Y the LINK to X's FREQ. A click picks one mode; Shift+click combines
 * modes (`pickSwitches`).
 */
import type { FilterUnit as Unit } from "../../lib/ipc";
import { pickSwitches } from "../../lib/mixView";
import type { FilterValues } from "../../store/mix";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { Tooltip } from "../ui/Tooltip";
import { LabelledKnob } from "./LabelledKnob";

export interface FilterUnitProps {
  unit: Unit;
  values: FilterValues;
  onChange: <K extends keyof FilterValues>(param: K, value: FilterValues[K]) => void;
}

type Mode = "highPass" | "bandPass" | "lowPass";

/** `[param, button text, spoken name]`. */
const MODES: [Mode, string, string][] = [
  ["highPass", "HP", "high-pass"],
  ["bandPass", "BP", "band-pass"],
  ["lowPass", "LP", "low-pass"],
];

export function FilterUnit({ unit, values, onChange }: FilterUnitProps) {
  const name = `Filter ${unit.toUpperCase()}`;
  const toggle = (param: "on" | "vcfToXfader" | "link", text: string, spoken: string) => (
    <Button
      key={param}
      variant="control"
      size="xs"
      aria-pressed={values[param]}
      aria-label={`${name} ${spoken}`}
      onClick={() => onChange(param, !values[param])}
    >
      {text}
    </Button>
  );
  const pickMode = (i: number, additive: boolean) => {
    const was = MODES.map(([p]) => values[p]);
    pickSwitches(was, i, additive, true).forEach((on, j) => {
      const mode = MODES[j]?.[0];
      if (mode !== undefined && on !== was[j]) onChange(mode, on);
    });
  };
  return (
    <section aria-label={name} className="flex flex-col items-center gap-1.5">
      <div className="flex items-center gap-1.5">
        <Caption tone="default">{name}</Caption>
        {toggle("on", "On", "on")}
      </div>
      <div className="flex items-end gap-2">
        <LabelledKnob
          caption="Freq"
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
      </div>
      <div className="flex gap-0.5">
        {MODES.map(([p, text, spoken], i) => (
          <Tooltip key={p} content="Shift+click to combine modes">
            <Button
              variant="control"
              size="xs"
              aria-pressed={values[p]}
              aria-label={`${name} ${spoken}`}
              onClick={(e) => pickMode(i, e.shiftKey)}
            >
              {text}
            </Button>
          </Tooltip>
        ))}
      </div>
      <div className="flex gap-0.5">
        {toggle("vcfToXfader", "VCF", "follows the crossfader")}
        {unit === "y" && toggle("link", "Link", "linked to X")}
      </div>
    </section>
  );
}
