/**
 * A small value box in the controls' size scale (mono, tabular figures):
 * the loop size, and anywhere else a deck shows a number. `dimmed` is a
 * value that is not in effect yet (the size LOOP will use next).
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { controlSizes } from "./control";

export const readoutVariants = cva(
  "inline-flex min-w-0 items-center justify-center border border-border bg-surface font-mono font-semibold tabular-nums whitespace-nowrap",
  {
    variants: {
      size: controlSizes,
      dimmed: { true: "text-fg-subtle", false: "text-fg" },
    },
    defaultVariants: { size: "sm", dimmed: false },
  },
);

export type ReadoutProps = ComponentProps<"span"> &
  Omit<VariantProps<typeof readoutVariants>, "dimmed"> & { dimmed?: boolean };

export function Readout({ size, dimmed = false, className, ...props }: ReadoutProps) {
  return (
    // role="img": the label names the value in full ("next loop size: 4"), the box shows it short.
    <span
      role="img"
      data-dimmed={dimmed || undefined}
      className={cn(readoutVariants({ size, dimmed }), className)}
      {...props}
    />
  );
}
