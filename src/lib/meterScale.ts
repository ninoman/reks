/**
 * The level meter's LED scale, as on a DJM channel meter: nine LEDs from
 * −20 to +10. Pure, shared by the meter component and the store selector
 * (which subscribes to the lit count, not the raw level, so a meter renders
 * only when an LED changes).
 */

/**
 * Scale dB = dBFS + this: +10 (the red LED) is 0 dBFS, 0 (blue) is −10 dBFS,
 * leaving 10 dB of headroom above the reference. Our choice; how a DJM's
 * meter maps to its digital headroom is UNVERIFIED.
 */
export const METER_OFFSET_DB = 10;

export type LedTone = "clip" | "hot" | "ref" | "normal";

export interface Led {
  /** Scale dB at which the LED lights. */
  db: number;
  label: string;
  tone: LedTone;
}

/** Bottom to top. */
export const METER_LEDS: readonly Led[] = [
  { db: -20, label: "-20", tone: "normal" },
  { db: -15, label: "-15", tone: "normal" },
  { db: -10, label: "-10", tone: "normal" },
  { db: -6, label: "-6", tone: "normal" },
  { db: -3, label: "-3", tone: "normal" },
  { db: 0, label: "0", tone: "ref" },
  { db: 3, label: "+3", tone: "hot" },
  { db: 6, label: "+6", tone: "hot" },
  { db: 10, label: "+10", tone: "clip" },
];

/** Linear level (1 = 0 dBFS) to scale dB; −∞ for silence. */
export function scaleDb(level: number): number {
  return level > 0 ? 20 * Math.log10(level) + METER_OFFSET_DB : Number.NEGATIVE_INFINITY;
}

/** How many LEDs are lit, from the bottom, for a linear level. */
export function litCount(level: number): number {
  const db = scaleDb(level);
  let n = 0;
  while (n < METER_LEDS.length && db >= (METER_LEDS[n] as Led).db) n++;
  return n;
}
