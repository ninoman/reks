/**
 * One mixer channel, top to bottom as on the hardware: level (trim), HI /
 * HM / LM / LO EQ, filter assign, cue, fader, crossfader assign. Rows
 * line up with `ChannelLegend`, which carries the captions once.
 */
import type { ChannelValues } from "../../store/mix";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { Fader } from "../ui/Fader";
import { Knob } from "../ui/Knob";
import { MeterScale } from "../ui/Meter";
import { Tooltip } from "../ui/Tooltip";
import { AssignSwitch } from "./AssignSwitch";
import { LiveMeter } from "./LiveMeter";

type EqParam = "eqHi" | "eqMid1" | "eqMid2" | "eqLo";

const KNOBS: ["trim" | EqParam, string][] = [
  ["trim", "Level"],
  ["eqHi", "Hi"],
  ["eqMid1", "HM"],
  ["eqMid2", "LM"],
  ["eqLo", "Lo"],
];

/** Row captions, in strip order (the fader row has none). */
const STRIP_ROWS = [...KNOBS.map(([, c]) => c), "Filter", "Cue", "", "X-F"];

const row = "flex h-(--mixer-row-h) items-center justify-center";

export interface ChannelStripProps {
  ch: number;
  values: ChannelValues;
  /** The deck on this channel has a track. */
  active: boolean;
  onChange: <K extends keyof ChannelValues>(param: K, value: ChannelValues[K]) => void;
  /** CUE pressed; `additive` is Shift+click (cue this channel on top of others). */
  onCue: (additive: boolean) => void;
}

export function ChannelStrip({ ch, values, active, onChange, onCue }: ChannelStripProps) {
  const n = ch + 1;
  return (
    <section aria-label={`Channel ${n}`} className="flex w-(--mixer-strip-w) flex-col items-center">
      <Badge variant={active ? "accent" : "muted"} size="md" className="rounded-xs font-mono">
        {n}
      </Badge>
      {KNOBS.map(([param, caption]) => (
        <div key={param} className={row}>
          <Knob
            size={param === "trim" ? "sm" : "md"}
            bipolar
            defaultValue={0.5}
            value={values[param]}
            onValueChange={(v) => onChange(param, v)}
            aria-label={`Channel ${n} ${param === "trim" ? "level" : `EQ ${caption}`}`}
          />
        </div>
      ))}
      <div className={row}>
        <AssignSwitch
          label={`Channel ${n} filter assign`}
          value={values.filterAssign}
          onChange={(v) => onChange("filterAssign", v)}
        />
      </div>
      <div className={row}>
        <Tooltip content="Shift+click to cue more than one channel">
          <Button
            variant="control"
            size="xs"
            aria-pressed={values.cue}
            aria-label={`Channel ${n} cue`}
            onClick={(e) => onCue(e.shiftKey)}
          >
            Cue
          </Button>
        </Tooltip>
      </div>
      <div className="my-1 flex gap-1.5">
        <Fader
          value={values.fader}
          defaultValue={1}
          onValueChange={(v) => onChange("fader", v)}
          aria-label={`Channel ${n} fader`}
        />
        <LiveMeter source={ch} labels={false} aria-label={`Channel ${n} meter`} />
      </div>
      <div className={row}>
        <AssignSwitch
          label={`Channel ${n} crossfader assign`}
          value={values.xfadeAssign}
          onChange={(v) => onChange("xfadeAssign", v)}
        />
      </div>
    </section>
  );
}

/** The captions for every strip's rows and the dB scale for their meters, once, on the left. */
export function ChannelLegend() {
  return (
    <div aria-hidden="true" className="flex min-w-0 flex-1 flex-col items-end pr-1">
      {/* The header row: the mixer's settings button sits here. */}
      <span className="h-5" />
      {STRIP_ROWS.map((caption, i) =>
        caption === "" ? (
          // The fader row: the one dB scale, as tall as the fader.
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed rows
          <MeterScale key={i} className="my-1" />
        ) : (
          <Caption key={caption} className="flex h-(--mixer-row-h) items-center">
            {caption}
          </Caption>
        ),
      )}
    </div>
  );
}
