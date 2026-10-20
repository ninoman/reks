/**
 * Sync phase: how far a deck's beat is from the leader's, as a marker on a
 * half-beat-either-side scale (bar sync: two beats either side). Green when
 * within 1/32 beat.
 */
import { cn } from "../../lib/cn";
import { PHASE_LOCK } from "../../lib/mixView";
import { Caption } from "../ui/Caption";

export interface SyncPhaseProps {
  /** Beats ahead (+) or behind (−) the leader; null when there is nothing to compare. */
  offset: number | null;
  /** Half the scale, in beats: 0.5, or 2 for bar phase. */
  half: number;
  leader: boolean;
}

export function SyncPhase({ offset, half, leader }: SyncPhaseProps) {
  if (leader) {
    return (
      <Caption tone="default" className="w-16 text-center">
        Leader
      </Caption>
    );
  }
  const inPhase = offset !== null && Math.abs(offset) <= PHASE_LOCK;
  const label =
    offset === null
      ? "Phase: no leader or grid"
      : `Phase: ${offset >= 0 ? "+" : "−"}${Math.abs(offset).toFixed(2)} beat`;
  return (
    <div
      role="img"
      aria-label={label}
      title={label}
      className="relative h-2 w-16 rounded-full bg-surface-strong"
    >
      <span className="absolute inset-y-0 left-1/2 w-px bg-border-strong" />
      {offset !== null && (
        <span
          data-in-phase={inPhase || undefined}
          className={cn(
            "absolute inset-y-0 w-1 -translate-x-1/2 rounded-full bg-warning",
            inPhase && "bg-success",
          )}
          style={{ left: `${50 + (offset / half) * 50}%` }}
        />
      )}
    </div>
  );
}
