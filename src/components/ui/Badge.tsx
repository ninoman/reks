/** A small count or label pill. */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const badgeVariants = cva(
  "inline-flex shrink-0 items-center justify-center rounded-full font-semibold tabular-nums",
  {
    variants: {
      variant: {
        neutral: "bg-surface-strong text-fg",
        accent: "bg-accent-solid text-accent-fg",
        muted: "bg-surface-strong text-fg-muted",
        /** A word on a row, e.g. the Filters drawer's "not" / "all". */
        danger: "text-danger ring-1 ring-danger ring-inset",
        outline: "text-accent ring-1 ring-accent ring-inset",
      },
      size: {
        sm: "h-4 min-w-4 px-1 text-2xs leading-none",
        md: "h-5 min-w-5 px-1.5 text-xs leading-none",
      },
    },
    defaultVariants: { variant: "neutral", size: "sm" },
  },
);

export type BadgeProps = ComponentProps<"span"> & VariantProps<typeof badgeVariants>;

export function Badge({ variant, size, className, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant, size }), className)} {...props} />;
}
