/**
 * CueFlag: a hot cue's letter on its colour, marking where the cue sits on a
 * waveform thumbnail. Place it in a positioned parent at the cue's offset;
 * its left edge is the cue. Decorative: the parent carries the label.
 */
import type { CSSProperties } from "react";

import { cn } from "../../lib/cn";

export function CueFlag({
  label,
  color,
  left,
  className,
}: {
  /** The slot's letter. */
  label: string;
  /** CSS colour; the default hot cue colour when undefined. */
  color?: string | undefined;
  /** Offset from the parent's left edge, CSS length. */
  left: string;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute top-0 flex h-(--cue-flag-size) min-w-(--cue-flag-size) items-center justify-center",
        "rounded-2xs bg-(--cue-flag-color) px-px text-(length:--cue-flag-text) font-bold leading-none text-cue-flag-fg",
        className,
      )}
      style={
        {
          left,
          "--cue-flag-color": color ?? "var(--color-deck-hotcue)",
        } as CSSProperties
      }
    >
      {label}
    </span>
  );
}
