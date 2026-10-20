/**
 * One mixer channel, top to bottom as on the hardware: level (trim), HI /
 * MID 1 / MID 2 / LO EQ, filter assign, cue, fader, crossfader assign. Rows
 * line up with `ChannelLegend`, which carries the captions once.
 */
import type { ChannelValues } from "../../store/mix";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { Fader } from "../ui/Fader";
import { Knob } from "../ui/Knob";
import { AssignSwitch } from "./AssignSwitch";

type EqParam = "eqHi" | "eqMid1" | "eqMid2" | "eqLo";

const KNOBS: ["trim" | EqParam, string][] = [
  ["trim", "Level"],
  ["eqHi", "Hi"],
  ["eqMid1", "Mid 1"],
  ["eqMid2", "Mid 2"],
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
}

export function ChannelStrip({ ch, values, active, onChange }: ChannelStripProps) {
  const n = ch + 1;
  return (
    <section aria-label={`Channel ${n}`} className="flex w-(--mixer-strip-w) flex-col items-center">
      <Caption tone={active ? "default" : "subtle"} className="h-4">
        {n}
      </Caption>
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
        <Button
          size="xs"
          aria-pressed={values.cue}
          aria-label={`Channel ${n} cue`}
          onClick={() => onChange("cue", !values.cue)}
        >
          Cue
        </Button>
      </div>
      <Fader
        className="my-1"
        value={values.fader}
        defaultValue={1}
        onValueChange={(v) => onChange("fader", v)}
        aria-label={`Channel ${n} fader`}
      />
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

/** The captions for every strip's rows, once, on the left. */
export function ChannelLegend() {
  return (
    <div aria-hidden="true" className="flex flex-col items-end pr-1">
      <span className="h-4" />
      {STRIP_ROWS.map((caption, i) =>
        caption === "" ? (
          // The fader row: as tall as the fader and its margin.
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed rows
          <span key={i} className="my-1 h-(--fader-len)" />
        ) : (
          <Caption key={caption} className="flex h-(--mixer-row-h) items-center">
            {caption}
          </Caption>
        ),
      )}
    </div>
  );
}
