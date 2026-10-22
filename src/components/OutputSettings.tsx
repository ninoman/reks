/**
 * Settings → Playback: how the audio output opens. Output device, sample
 * rate, buffer and headphone cue, each saved (`chooseOutput`); a running
 * output reopens with the change and the loaded tracks carry on where they
 * were. Rates, buffer sizes and cue routings come from what the chosen
 * device offers (`MixStore.options`).
 */
import { useEffect } from "react";

import type { CueRouting } from "../lib/ipc";
import { cueFits } from "../lib/mixView";
import { useMix } from "../store/mix";
import { chooseOutput } from "../store/settings";
import { Field } from "./ui/Field";
import { Select } from "./ui/Select";

const CUE_CHOICES: [CueRouting["type"], CueRouting, string][] = [
  ["off", { type: "off" }, "Off (master only)"],
  ["splitMono", { type: "splitMono" }, "Split: cue left, master right"],
  ["outputs", { type: "outputs", master: [0, 1], cue: [2, 3] }, "Master on 1/2, cue on 3/4"],
];

const BUFFERS = [64, 128, 256, 512, 1024, 2048];

/** `""` in a select means "not chosen" (`null`). */
const numberOrNull = (v: string) => (v === "" ? null : Number(v));

export function OutputSettings({ itemClassName }: { itemClassName: string }) {
  const config = useMix((s) => s.config);
  const devices = useMix((s) => s.devices);
  const options = useMix((s) => s.options);
  useEffect(() => {
    void useMix.getState().loadDevices();
  }, []);

  const device = config.device;
  // A chosen device that is not connected stays listed so the choice shows.
  const deviceList = device !== null && !devices.includes(device) ? [device, ...devices] : devices;
  const [min, max] = options?.bufferFrames ?? [0, Number.POSITIVE_INFINITY];
  const buffers = BUFFERS.filter((b) => b >= min && b <= max);
  const select = "min-w-55";

  return (
    <>
      <div className={itemClassName}>
        <Field label="Output device" orientation="horizontal">
          <Select
            className={select}
            value={device ?? ""}
            onChange={(e) => void chooseOutput({ device: e.target.value || null })}
          >
            <option value="">System default</option>
            {deviceList.map((d) => (
              <option key={d} value={d}>
                {d}
                {devices.includes(d) ? "" : " (not connected)"}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className={itemClassName}>
        <Field
          label="Sample rate"
          orientation="horizontal"
          hint="Automatic is 44.1 kHz when the device offers it, as most DJ music is."
        >
          <Select
            className={select}
            value={config.sampleRate ?? ""}
            onChange={(e) => void chooseOutput({ sampleRate: numberOrNull(e.target.value) })}
          >
            <option value="">Automatic</option>
            {(options?.rates ?? []).map((r) => (
              <option key={r} value={r}>
                {(r / 1000).toFixed(1)} kHz
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className={itemClassName}>
        <Field
          label="Buffer"
          orientation="horizontal"
          hint="Smaller reacts faster; raise it if you hear dropouts."
        >
          <Select
            className={select}
            value={config.bufferFrames ?? ""}
            onChange={(e) => void chooseOutput({ bufferFrames: numberOrNull(e.target.value) })}
          >
            <option value="">Device default</option>
            {buffers.map((b) => (
              <option key={b} value={b}>
                {b} frames
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className={itemClassName}>
        <Field
          label="Headphone cue"
          orientation="horizontal"
          hint="Where the Mix panel's CUE buttons send a channel."
        >
          <Select
            className={select}
            value={config.cue.type}
            onChange={(e) => {
              const choice = CUE_CHOICES.find(([k]) => k === e.target.value);
              if (choice !== undefined) void chooseOutput({ cue: choice[1] });
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
      </div>
    </>
  );
}
