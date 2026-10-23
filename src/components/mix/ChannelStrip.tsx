/**
 * One mixer channel, top to bottom as on the hardware: level (trim), the EQ
 * (4-band HI / HM / LM / LO, or 3-band HI / MID / LOW and the FX knob),
 * filter assign, cue, fader, crossfader assign. Rows line up with
 * `ChannelLegend`, which carries the captions once. A knob off centre is lit.
 */
import { cn } from "../../lib/cn";
import type { FxKind } from "../../lib/ipc";
import { FX_DEAD_ZONE, fxName, type MixerParts, offCentre } from "../../lib/mixView";
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

type KnobParam =
  | "trim"
  | "eqHi"
  | "eqMid1"
  | "eqMid2"
  | "eqLo"
  | "eq3Hi"
  | "eq3Mid"
  | "eq3Lo"
  | "fx";

/** `[param, caption]` per knob row; the FX row's caption is the selected effect. */
const FOUR_BAND: [KnobParam, string][] = [
  ["trim", "Level"],
  ["eqHi", "Hi"],
  ["eqMid1", "HM"],
  ["eqMid2", "LM"],
  ["eqLo", "Lo"],
];
const THREE_BAND: [KnobParam, string][] = [
  ["trim", "Level"],
  ["eq3Hi", "Hi"],
  ["eq3Mid", "Mid"],
  ["eq3Lo", "Low"],
  ["fx", "FX"],
];

function knobRows(parts: MixerParts): [KnobParam, string][] {
  return parts.threeBand ? THREE_BAND : FOUR_BAND;
}

function ariaName(param: KnobParam, caption: string): string {
  if (param === "trim") return "level";
  if (param === "fx") return "FX";
  return `EQ ${caption}`;
}

const row = "flex h-(--mixer-row-h) items-center justify-center";

export interface ChannelStripProps {
  ch: number;
  values: ChannelValues;
  /** The deck on this channel has a track. */
  active: boolean;
  parts: MixerParts;
  onChange: <K extends keyof ChannelValues>(param: K, value: ChannelValues[K]) => void;
  /** CUE pressed; `additive` is Shift+click (cue this channel on top of others). */
  onCue: (additive: boolean) => void;
}

export function ChannelStrip({ ch, values, active, parts, onChange, onCue }: ChannelStripProps) {
  const n = ch + 1;
  return (
    <section
      aria-label={`Channel ${n}`}
      className="flex w-(--mixer-strip-w) flex-col items-center justify-between"
    >
      <Badge variant={active ? "accent" : "muted"} size="md" className="rounded-xs font-mono">
        {n}
      </Badge>
      {knobRows(parts).map(([param, caption]) => {
        const fx = param === "fx";
        return (
          <div key={param} className={row}>
            <Knob
              size={param === "trim" ? "sm" : "md"}
              bipolar
              defaultValue={0.5}
              {...(fx && { detent: 0.5 })}
              lit={offCentre(values[param], fx ? FX_DEAD_ZONE : 0)}
              value={values[param]}
              onValueChange={(v) => onChange(param, v)}
              aria-label={`Channel ${n} ${ariaName(param, caption)}`}
            />
          </div>
        );
      })}
      {parts.filterAssigns && (
        <div className={row}>
          <AssignSwitch
            label={`Channel ${n} filter assign`}
            value={values.filterAssign}
            onChange={(v) => onChange("filterAssign", v)}
          />
        </div>
      )}
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
      <div className={cn("flex gap-1.5", parts.crossfader ? "my-1" : "my-0.5")}>
        <Fader
          value={values.fader}
          defaultValue={1}
          onValueChange={(v) => onChange("fader", v)}
          aria-label={`Channel ${n} fader`}
        />
        <LiveMeter source={ch} labels={false} aria-label={`Channel ${n} meter`} />
      </div>
      {parts.crossfader && (
        <div className={row}>
          <AssignSwitch
            label={`Channel ${n} crossfader assign`}
            value={values.xfadeAssign}
            onChange={(v) => onChange("xfadeAssign", v)}
          />
        </div>
      )}
    </section>
  );
}

/** Row captions, in strip order; "" is the fader row. */
function legendRows(parts: MixerParts, fx: FxKind): string[] {
  const knobs = knobRows(parts).map(([param, c]) => (param === "fx" ? fxName(fx) : c));
  return [
    ...knobs,
    ...(parts.filterAssigns ? ["Filter"] : []),
    "Cue",
    "",
    ...(parts.crossfader ? ["X-F"] : []),
  ];
}

/** The captions for every strip's rows and the dB scale for their meters, once, on the left. */
export function ChannelLegend({ parts, fx }: { parts: MixerParts; fx: FxKind }) {
  const scaleGap = parts.crossfader ? "my-1" : "my-0.5";
  return (
    <div aria-hidden="true" className="flex shrink-0 flex-col items-end justify-between pr-1">
      {/* The header row: the mixer's settings button sits here. */}
      <span className="h-5" />
      {legendRows(parts, fx).map((caption, i) =>
        caption === "" ? (
          // The fader row: the one dB scale, as tall as the fader.
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed rows
          <MeterScale key={i} className={scaleGap} />
        ) : (
          <Caption key={caption} className="flex h-(--mixer-row-h) items-center">
            {caption}
          </Caption>
        ),
      )}
    </div>
  );
}
