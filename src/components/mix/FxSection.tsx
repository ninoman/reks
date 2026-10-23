/**
 * The 3-band mixer's FX row: FX select (one effect for every channel's FX
 * knob, exactly one lit) and PARAMETER, captioned by what it sets for that
 * effect.
 */
import { FX_KINDS, fxName, fxParameterName } from "../../lib/mixView";
import type { FxValues } from "../../store/mix";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { Knob } from "../ui/Knob";

export interface FxSectionProps {
  values: FxValues;
  onChange: (patch: Partial<FxValues>) => void;
}

export function FxSection({ values, onChange }: FxSectionProps) {
  const parameter = fxParameterName(values.kind);
  return (
    <section
      aria-label="FX"
      className="flex items-center gap-1.5 border-t border-border-subtle px-1 pt-1"
    >
      <Caption tone="default">FX</Caption>
      <div className="flex gap-0.5">
        {FX_KINDS.map((kind) => (
          <Button
            key={kind}
            variant="control"
            size="xs"
            aria-pressed={values.kind === kind}
            onClick={() => onChange({ kind })}
          >
            {fxName(kind)}
          </Button>
        ))}
      </div>
      <div className="ml-auto flex items-center gap-1.5">
        <Caption aria-hidden="true">{parameter}</Caption>
        <Knob
          defaultValue={0.5}
          value={values.parameter}
          onValueChange={(v) => onChange({ parameter: v })}
          aria-label={`FX ${parameter.toLowerCase()}`}
        />
      </div>
    </section>
  );
}
