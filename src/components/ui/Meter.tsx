/**
 * A vertical peak meter, as long as a fader. `level` is linear (1 = 0 dBFS)
 * and shown on a dB scale from `FLOOR_DB`; the cap lights when `clip` is set.
 */
import { cn } from "../../lib/cn";

/** Bottom of the scale, in dBFS. */
export const FLOOR_DB = -48;
/** Above this the bar turns to the warning colour. */
const HOT_DB = -6;

/** Linear level to dBFS, `FLOOR_DB` for silence. */
export function levelDb(level: number): number {
  return level > 0 ? Math.max(FLOOR_DB, 20 * Math.log10(level)) : FLOOR_DB;
}

export interface MeterProps {
  level: number;
  clip: boolean;
  "aria-label": string;
  className?: string;
}

export function Meter({ level, clip, className, ...aria }: MeterProps) {
  const db = Math.min(0, levelDb(level));
  const fill = (db - FLOOR_DB) / -FLOOR_DB;
  return (
    // biome-ignore lint/a11y/useSemanticElements: a native <meter> can't be drawn vertically with a clip cap across engines
    <div
      role="meter"
      aria-valuemin={FLOOR_DB}
      aria-valuemax={0}
      aria-valuenow={Math.round(db)}
      aria-valuetext={clip ? "Clipping" : `${Math.round(db)} dB`}
      className={cn("flex h-(--fader-len) w-(--meter-w) flex-col gap-px", className)}
      {...aria}
    >
      <span className={cn("h-1 shrink-0 rounded-xs", clip ? "bg-danger" : "bg-surface-strong")} />
      <span className="relative flex-1 overflow-hidden rounded-xs bg-surface-strong">
        <span
          className={cn("absolute inset-x-0 bottom-0", db > HOT_DB ? "bg-warning" : "bg-success")}
          style={{ height: `${fill * 100}%` }}
        />
      </span>
    </div>
  );
}
