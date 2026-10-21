/**
 * Opening the mixer's audio output: device, sample rate, buffer size and
 * where the headphone cue goes.
 */
import type { CueRouting, MixConfig, MixDeviceOptions } from "../../lib/ipc";
import { cueFits } from "../../lib/mixView";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Select } from "../ui/Select";

export interface MixStartFormProps {
  config: MixConfig;
  devices: string[];
  options: MixDeviceOptions | null;
  starting: boolean;
  onChange: (patch: Partial<MixConfig>) => void;
  onStart: () => void;
}

const CUE_CHOICES: [string, CueRouting, string][] = [
  ["off", { type: "off" }, "Off (master only)"],
  ["splitMono", { type: "splitMono" }, "Split: cue left, master right"],
  ["outputs", { type: "outputs", master: [0, 1], cue: [2, 3] }, "Master on 1/2, cue on 3/4"],
];

const BUFFERS = [64, 128, 256, 512, 1024, 2048];

export function MixStartForm({
  config,
  devices,
  options,
  starting,
  onChange,
  onStart,
}: MixStartFormProps) {
  const [min, max] = options?.bufferFrames ?? [0, Number.POSITIVE_INFINITY];
  const buffers = BUFFERS.filter((b) => b >= min && b <= max);
  return (
    <form
      className="flex flex-wrap items-end gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        onStart();
      }}
    >
      <Field label="Output">
        <Select
          value={config.device ?? ""}
          onChange={(e) => onChange({ device: e.target.value === "" ? null : e.target.value })}
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </Select>
      </Field>
      <Field label="Sample rate">
        <Select
          value={config.sampleRate ?? ""}
          onChange={(e) =>
            onChange({ sampleRate: e.target.value === "" ? null : Number(e.target.value) })
          }
        >
          <option value="">Automatic</option>
          {(options?.rates ?? []).map((r) => (
            <option key={r} value={r}>
              {(r / 1000).toFixed(1)} kHz
            </option>
          ))}
        </Select>
      </Field>
      <Field label="Buffer">
        <Select
          value={config.bufferFrames ?? ""}
          onChange={(e) =>
            onChange({ bufferFrames: e.target.value === "" ? null : Number(e.target.value) })
          }
        >
          <option value="">Device default</option>
          {buffers.map((b) => (
            <option key={b} value={b}>
              {b} frames
            </option>
          ))}
        </Select>
      </Field>
      <Field label="Headphone cue">
        <Select
          value={config.cue.type}
          onChange={(e) => {
            const choice = CUE_CHOICES.find(([k]) => k === e.target.value);
            if (choice !== undefined) onChange({ cue: choice[1] });
          }}
        >
          {CUE_CHOICES.map(([k, routing, label]) => {
            // Unknown outputs until the options arrive: offer everything.
            const fits = options === null || cueFits(routing, options.channels);
            return (
              <option key={k} value={k} disabled={!fits}>
                {fits ? label : `${label} (needs ${k === "outputs" ? 4 : 2} outputs)`}
              </option>
            );
          })}
        </Select>
      </Field>
      <Button type="submit" variant="primary" disabled={starting}>
        {starting ? "Starting…" : "Start output"}
      </Button>
    </form>
  );
}
