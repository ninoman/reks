/**
 * Filter unit X or Y: FREQ, RESONANCE, HP / BP / LP and the ON lamp. A click picks one mode;
 * Shift+click combines modes (`pickSwitches`). The unit on the `right` is the mirror image of
 * the one on the left, around the mixer's centre line.
 */
import { cn } from "../../lib/cn";
import type { FilterUnit as Unit } from "../../lib/ipc";
import { pickSwitches } from "../../lib/mixView";
import type { FilterValues } from "../../store/mix";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { LampButton } from "../ui/LampButton";
import { Tooltip } from "../ui/Tooltip";
import { LabelledKnob } from "./LabelledKnob";

export interface FilterUnitProps {
  unit: Unit;
  side: "left" | "right";
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

export function FilterUnit({ unit, side, values, onChange }: FilterUnitProps) {
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
    <section aria-label={name} className={cn("flex items-center gap-2", mirror)}>
      <div className={cn("flex items-end gap-1.5", mirror)}>
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
      </div>
      <div className={cn("flex flex-col gap-1", side === "right" && "items-end")}>
        <div className={cn("flex items-center gap-1.5", mirror)}>
          <LampButton
            pressed={values.on}
            aria-label={`${name} on`}
            onClick={() => onChange("on", !values.on)}
          />
          <Caption tone="default">{name}</Caption>
        </div>
        <div className={cn("flex gap-0.5", mirror)}>
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
      </div>
    </section>
  );
}
