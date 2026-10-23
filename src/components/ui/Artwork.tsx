/**
 * A track's artwork: the picture, or a record-glyph tile without one. `size`:
 * `fill` (the info panel: full width, square), `deck` (`--deck-art`) or `row`
 * (`--row-art`, a track-list thumbnail that loads the small cached JPEG
 * instead of the full picture). `action` puts a button over it that shows on
 * hover or keyboard focus (the deck's eject), or always with `actionShown`.
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ReactNode } from "react";

import { cn } from "../../lib/cn";
import { useArtworkUrl } from "../useArtworkUrl";

const artworkVariants = cva("relative shrink-0 overflow-hidden rounded-xs", {
  variants: {
    size: {
      fill: "aspect-square w-full",
      deck: "size-(--deck-art)",
      row: "size-(--row-art)",
    },
  },
  defaultVariants: { size: "fill" },
});

export interface ArtworkProps extends VariantProps<typeof artworkVariants> {
  artworkId: number | null;
  /** Mime of the artwork bytes ("" when unknown). */
  mime?: string;
  /** A button shown over the artwork on hover or focus. */
  action?: ReactNode;
  /** Keep `action` shown without hover (e.g. while it is playing). */
  actionShown?: boolean;
  className?: string;
}

/** A vinyl record: outer edge, one groove, label and spindle hole. */
function RecordGlyph() {
  return (
    <svg viewBox="0 0 24 24" fill="none" className="size-3/5" aria-hidden="true">
      <circle cx="12" cy="12" r="10" fill="currentColor" opacity="0.35" />
      <circle cx="12" cy="12" r="6.5" stroke="currentColor" strokeWidth="0.8" opacity="0.6" />
      <circle cx="12" cy="12" r="3.2" fill="currentColor" />
      <circle cx="12" cy="12" r="0.9" fill="var(--color-surface-strong)" />
    </svg>
  );
}

export function Artwork({
  artworkId,
  mime = "",
  size,
  action,
  actionShown = false,
  className,
}: ArtworkProps) {
  const url = useArtworkUrl(artworkId, mime, size === "row");
  return (
    <div className={cn(artworkVariants({ size }), "group/art", className)}>
      {url === null ? (
        <div
          className="flex size-full items-center justify-center rounded-xs border border-border bg-surface-strong text-fg-subtle"
          role="img"
          aria-label="No artwork"
        >
          <RecordGlyph />
        </div>
      ) : (
        <img
          className="size-full rounded-xs object-cover"
          src={url}
          alt={size === "row" ? "" : "Artwork"}
          decoding="async"
          draggable={false}
        />
      )}
      {action !== undefined && (
        <div
          className="absolute inset-0 opacity-0 group-hover/art:opacity-100 focus-within:opacity-100 data-[shown]:opacity-100"
          data-shown={actionShown || undefined}
        >
          {action}
        </div>
      )}
    </div>
  );
}
