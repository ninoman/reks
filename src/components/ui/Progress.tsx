/** Determinate or (without `value`) indeterminate progress: a native <progress>. */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const progressVariants = cva(
  [
    "block w-full appearance-none overflow-hidden rounded-full border-0 bg-surface-strong",
    "[&::-webkit-progress-bar]:bg-surface-strong [&::-webkit-progress-value]:bg-accent [&::-moz-progress-bar]:bg-accent",
  ],
  { variants: { size: { sm: "h-1", md: "h-1.5" } }, defaultVariants: { size: "md" } },
);

export type ProgressProps = ComponentProps<"progress"> & VariantProps<typeof progressVariants>;

export function Progress({ size, className, ...props }: ProgressProps) {
  return <progress className={cn(progressVariants({ size }), className)} {...props} />;
}
