/**
 * A vertical LED level meter, as long as a fader: a column of round LEDs on
 * the deck colour (`METER_LEDS`, −20 to +10), lit ones glowing, with the dB
 * labels beside them unless `labels` is off (a shared `MeterScale` serves several). Takes the lit count rather than the level, so a live
 * meter re-renders only when an LED changes; the top (red) LED also lights
 * while `clip` is held.
 */
import { cn } from "../../lib/cn";
import { type LedTone, METER_LEDS } from "../../lib/meterScale";

const TONE: Record<LedTone, string> = {
  clip: "[--led:var(--color-led-clip)]",
  hot: "[--led:var(--color-led-hot)]",
  ref: "[--led:var(--color-led-ref)]",
  normal: "[--led:var(--color-led-normal)]",
};

/** Top to bottom, as drawn. */
const TOP_DOWN = [...METER_LEDS].reverse();
const FLOOR_DB = (METER_LEDS[0]?.db ?? 0) - 5;

export interface MeterProps {
  /** LEDs lit from the bottom, 0..9 (`litCount`). */
  lit: number;
  clip: boolean;
  /** Draw the dB labels beside the LEDs. */
  labels?: boolean;
  "aria-label": string;
  className?: string;
}

export function Meter({ lit, clip, labels = true, className, ...aria }: MeterProps) {
  const top = METER_LEDS[Math.min(lit, METER_LEDS.length) - 1];
  const db = top?.db ?? FLOOR_DB;
  return (
    // biome-ignore lint/a11y/useSemanticElements: a native <meter> can't be drawn as an LED column
    <div
      role="meter"
      aria-valuemin={FLOOR_DB}
      aria-valuemax={METER_LEDS.at(-1)?.db ?? 0}
      aria-valuenow={db}
      aria-valuetext={clip ? "Clipping" : top ? `${top.label} dB` : "Silent"}
      className={cn("flex h-(--fader-len) gap-0.5", className)}
      {...aria}
    >
      <span className="flex w-(--meter-w) flex-col items-center justify-around rounded-full bg-deck">
        {TOP_DOWN.map((led, i) => {
          const on = METER_LEDS.length - i <= lit || (led.tone === "clip" && clip);
          return (
            <span
              key={led.db}
              data-lit={on || undefined}
              className={cn(
                "size-(--meter-dot) rounded-full bg-(--led) opacity-25",
                "data-lit:opacity-100 data-lit:shadow-[0_0_5px_var(--led)]",
                TONE[led.tone],
              )}
            />
          );
        })}
      </span>
      {labels && <MeterScale />}
    </div>
  );
}

/** The dB labels alone, one per LED and as tall as a meter, so one column can serve every meter beside it. */
export function MeterScale({ className }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "flex h-(--fader-len) w-(--meter-label-w) flex-col justify-around text-(length:--meter-label-fs) text-fg-muted leading-none tabular-nums",
        className,
      )}
    >
      {TOP_DOWN.map((led) => (
        <span key={led.db}>{led.label}</span>
      ))}
    </span>
  );
}
