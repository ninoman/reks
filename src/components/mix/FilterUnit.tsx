/**
 * Filter unit X or Y: FREQ, RESONANCE, HP / BP / LP (any combination), ON,
 * VCF to crossfader, and on Y the LINK to X's FREQ.
 */
import type { FilterUnit as Unit } from "../../lib/ipc";
import type { FilterValues } from "../../store/mix";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { LabelledKnob } from "./LabelledKnob";

export interface FilterUnitProps {
  unit: Unit;
  values: FilterValues;
  onChange: <K extends keyof FilterValues>(param: K, value: FilterValues[K]) => void;
}

/** `[param, button text, spoken name]`. */
const MODES: ["highPass" | "bandPass" | "lowPass", string, string][] = [
  ["highPass", "HP", "high-pass"],
  ["bandPass", "BP", "band-pass"],
  ["lowPass", "LP", "low-pass"],
];

export function FilterUnit({ unit, values, onChange }: FilterUnitProps) {
  const name = `Filter ${unit.toUpperCase()}`;
  const toggle = (param: keyof FilterValues, text: string, spoken: string) => (
    <Button
      key={param}
      size="xs"
      aria-pressed={values[param] as boolean}
      aria-label={`${name} ${spoken}`}
      onClick={() => onChange(param, !values[param])}
    >
      {text}
    </Button>
  );
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
        {MODES.map(([p, text, spoken]) => toggle(p, text, spoken))}
      </div>
      <div className="flex gap-0.5">
        {toggle("vcfToXfader", "VCF", "follows the crossfader")}
        {unit === "y" && toggle("link", "Link", "linked to X")}
      </div>
    </section>
  );
}
