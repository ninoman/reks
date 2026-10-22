/**
 * Sync phase: how far a deck's beat is from the master's, as a marker on a
 * half-beat-either-side scale (bar sync: two beats either side), drawn on
 * the dark deck. Green when within 1/32 beat.
 */
import { cn } from "../../lib/cn";
import { PHASE_LOCK } from "../../lib/mixView";

export interface SyncPhaseProps {
  /** Beats ahead (+) or behind (−) the master; null when there is nothing to compare. */
  offset: number | null;
  /** Half the scale, in beats: 0.5, or 2 for bar phase. */
  half: number;
}

export function SyncPhase({ offset, half }: SyncPhaseProps) {
  const inPhase = offset !== null && Math.abs(offset) <= PHASE_LOCK;
  const label =
    offset === null
      ? "Phase: no master or grid"
      : `Phase: ${offset >= 0 ? "+" : "−"}${Math.abs(offset).toFixed(2)} beat`;
  return (
    <div
      role="img"
      aria-label={label}
      title={label}
      className="relative h-1.5 w-30 rounded-full bg-deck-fg/20"
    >
      <span className="absolute inset-y-0 left-1/2 w-px bg-deck-fg/50" />
      {offset !== null && (
        <span
          data-in-phase={inPhase || undefined}
          className={cn(
            "absolute -inset-y-px w-1 -translate-x-1/2 rounded-full bg-warning",
            inPhase && "bg-success",
          )}
          style={{ left: `${50 + (Math.max(-1, Math.min(1, offset / half))) * 50}%` }}
        />
      )}
    </div>
  );
}
