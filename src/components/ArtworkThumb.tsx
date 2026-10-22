/**
 * A track's artwork: the picture, or a placeholder without one. `size`:
 * `fill` (the info panel: full width, square) or `deck` (`--deck-art`, a
 * dashed box without artwork). `action` puts a button over it that shows on
 * hover or keyboard focus (the deck's eject).
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ReactNode } from "react";

import { cn } from "../lib/cn";
import { useArtworkUrl } from "./useArtworkUrl";

const thumbVariants = cva("relative shrink-0 overflow-hidden rounded-xs", {
  variants: {
    size: {
      fill: "aspect-square w-full",
      deck: "size-(--deck-art)",
    },
  },
  defaultVariants: { size: "fill" },
});

const placeholderVariants = cva("flex size-full items-center justify-center text-fg-muted", {
  variants: {
    size: {
      fill: "rounded-xs border border-border bg-bg text-[64px]",
      deck: "rounded-xs border border-dashed border-border-strong bg-surface-strong",
    },
  },
  defaultVariants: { size: "fill" },
});

export interface ArtworkThumbProps extends VariantProps<typeof thumbVariants> {
  artworkId: number | null;
  /** Mime of the artwork bytes ("" when unknown). */
  mime?: string;
  /** A button shown over the artwork on hover or focus. */
  action?: ReactNode;
  className?: string;
}

export function ArtworkThumb({ artworkId, mime = "", size, action, className }: ArtworkThumbProps) {
  const url = useArtworkUrl(artworkId, mime);
  return (
    <div className={cn(thumbVariants({ size }), "group/art", className)}>
      {url === null ? (
        <div className={placeholderVariants({ size })} role="img" aria-label="No artwork">
          {size === "deck" ? null : "♪"}
        </div>
      ) : (
        <img className="size-full rounded-xs object-cover" src={url} alt="Artwork" />
      )}
      {action !== undefined && (
        <div className="absolute inset-0 opacity-0 group-hover/art:opacity-100 focus-within:opacity-100">
          {action}
        </div>
      )}
    </div>
  );
}
