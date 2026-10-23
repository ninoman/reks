/**
 * The 3-band mixer's FX controls, in the channel legend: the FX row's
 * caption is the FX select (one effect for every channel's FX knob), and
 * PARAMETER sits under it, captioned by what it sets for that effect.
 */
import { FX_KINDS, fxName, fxParameterCaption, fxParameterName } from "../../lib/mixView";
import type { FxValues } from "../../store/mix";
import { Caption } from "../ui/Caption";
import { CaptionSelect } from "../ui/CaptionSelect";
import { Knob } from "../ui/Knob";

export interface FxControlProps {
  values: FxValues;
  onChange: (patch: Partial<FxValues>) => void;
}

const OPTIONS = FX_KINDS.map((kind) => ({
  value: kind,
  label: fxName(kind),
  hint: fxParameterName(kind),
}));

export function FxSelect({ values, onChange }: FxControlProps) {
  return (
    <CaptionSelect
      // The chevron uses the legend's right padding, so REVERB fits.
      className="-mr-1"
      value={values.kind}
      options={OPTIONS}
      onValueChange={(kind) => onChange({ kind })}
      aria-label="FX effect"
    />
  );
}

export function FxParameter({ values, onChange }: FxControlProps) {
  return (
    <div className="flex flex-col items-end gap-0.5">
      <Caption aria-hidden="true">{fxParameterCaption(values.kind)}</Caption>
      <Knob
        size="sm"
        defaultValue={0.5}
        value={values.parameter}
        onValueChange={(v) => onChange({ parameter: v })}
        aria-label={`FX ${fxParameterName(values.kind).toLowerCase()}`}
      />
    </div>
  );
}
