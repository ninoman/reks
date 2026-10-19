/** A keyboard shortcut, e.g. in a tooltip. */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const kbdVariants = cva(
  "inline-flex items-center justify-center rounded-xs border font-sans leading-none",
  {
    variants: {
      variant: {
        default: "border-border-strong bg-surface text-fg-muted",
        inverse: "border-transparent bg-bg/20 text-bg",
      },
      size: { sm: "h-4 min-w-4 px-1 text-2xs" },
    },
    defaultVariants: { variant: "default", size: "sm" },
  },
);

export type KbdProps = ComponentProps<"kbd"> & VariantProps<typeof kbdVariants>;

export function Kbd({ variant, size, className, ...props }: KbdProps) {
  return <kbd className={cn(kbdVariants({ variant, size }), className)} {...props} />;
}
