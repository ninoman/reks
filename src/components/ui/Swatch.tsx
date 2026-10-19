/**
 * A colour chip: a rekordbox track colour by name, or any colour passed in
 * `style` (cue colours from the library). Plain span, cheap in table rows.
 */
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

/** rekordbox track colour names (reks-core rbxml::TRACK_COLOURS) → token. */
const TRACK_COLOURS: Record<string, string> = {
  pink: "bg-track-pink",
  red: "bg-track-red",
  orange: "bg-track-orange",
  yellow: "bg-track-yellow",
  green: "bg-track-green",
  aqua: "bg-track-aqua",
  blue: "bg-track-blue",
  purple: "bg-track-purple",
};

export const swatchVariants = cva("inline-block shrink-0 align-middle", {
  variants: {
    size: { xs: "size-2 rounded-[2px]", sm: "size-2.5 rounded-[2px]", md: "size-3.5 rounded-xs" },
  },
  defaultVariants: { size: "sm" },
});

export type SwatchProps = ComponentProps<"span"> &
  VariantProps<typeof swatchVariants> & { color?: string; asChild?: boolean };

export function Swatch({ color, size, asChild = false, className, ...props }: SwatchProps) {
  const Comp = asChild ? Slot : "span";
  return (
    <Comp
      className={cn(
        swatchVariants({ size }),
        color !== undefined && (TRACK_COLOURS[color] ?? "bg-fg-subtle"),
        className,
      )}
      {...props}
    />
  );
}
