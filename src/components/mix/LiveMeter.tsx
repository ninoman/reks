/** A meter fed from the live engine state (`useMeter`). */
import { useMeter } from "../../store/mix";
import { Meter } from "../ui/Meter";

export interface LiveMeterProps {
  /** Channel 0..3, or the master. */
  source: number | "master";
  "aria-label": string;
}

export function LiveMeter({ source, ...aria }: LiveMeterProps) {
  const { level, clip } = useMeter(source);
  return <Meter level={level} clip={clip} {...aria} />;
}
