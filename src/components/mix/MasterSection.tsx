/**
 * Master and headphone cue: master level and meter, cue level, cue/mix
 * balance, split cue and post-EQ cue.
 */
import { cn } from "../../lib/cn";
import type { MasterValues } from "../../store/mix";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { LabelledKnob } from "./LabelledKnob";
import { LiveMeter } from "./LiveMeter";

export interface MasterSectionProps {
  values: MasterValues;
  /** The Simple mixer is shorter: tighter gaps, and the meter takes the height that is left, without its dB labels. */
  compact: boolean;
  onChange: <K extends keyof MasterValues>(param: K, value: MasterValues[K]) => void;
}

export function MasterSection({ values, compact, onChange }: MasterSectionProps) {
  return (
    <section
      aria-label="Master"
      className={cn(
        "ml-1 flex w-(--mixer-strip-w) flex-col items-center border-border-subtle border-l pl-1",
        compact ? "gap-0.5" : "gap-1",
      )}
    >
      <Caption tone="default">Master</Caption>
      <LabelledKnob
        caption="Level"
        value={values.level}
        defaultValue={0.8}
        onValueChange={(v) => onChange("level", v)}
        aria-label="Master level"
      />
      <LiveMeter
        source="master"
        aria-label="Master meter"
        labels={!compact}
        className={cn(compact && "h-auto min-h-(--meter-min-h) flex-1")}
      />
      <Caption tone="default" className={compact ? undefined : "mt-1"}>
        Cue
      </Caption>
      <LabelledKnob
        caption="Level"
        value={values.cueLevel}
        defaultValue={0.7}
        onValueChange={(v) => onChange("cueLevel", v)}
        aria-label="Cue level"
      />
      <LabelledKnob
        caption="Cue/Mix"
        bipolar
        value={values.cueMix}
        defaultValue={0}
        onValueChange={(v) => onChange("cueMix", v)}
        aria-label="Cue and mix balance"
      />
      <Button
        variant="control"
        size="xs"
        aria-pressed={values.splitCue}
        onClick={() => onChange("splitCue", !values.splitCue)}
      >
        Split
      </Button>
      <Button
        variant="control"
        size="xs"
        aria-pressed={values.cuePostEq}
        aria-label="Cue after EQ"
        onClick={() => onChange("cuePostEq", !values.cuePostEq)}
      >
        Post EQ
      </Button>
    </section>
  );
}
