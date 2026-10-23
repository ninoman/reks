/** A meter fed from the live engine state (`useMeter`). */
import { useMeter } from "../../store/mix";
import { Meter } from "../ui/Meter";

export interface LiveMeterProps {
  /** Channel 0..3, or the master. */
  source: number | "master";
  /** Draw the dB labels beside the LEDs. */
  labels?: boolean;
  className?: string;
  "aria-label": string;
}

export function LiveMeter({ source, ...aria }: LiveMeterProps) {
  const { lit, clip } = useMeter(source);
  return <Meter lit={lit} clip={clip} {...aria} />;
}
