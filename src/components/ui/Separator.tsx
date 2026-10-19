/** A decorative hairline between groups (hidden from assistive tech). */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const separatorVariants = cva("shrink-0 bg-border", {
  variants: {
    orientation: { horizontal: "h-px w-full", vertical: "w-px self-stretch" },
  },
  defaultVariants: { orientation: "horizontal" },
});

export type SeparatorProps = ComponentProps<"div"> & VariantProps<typeof separatorVariants>;

export function Separator({ orientation, className, ...props }: SeparatorProps) {
  return (
    <div role="none" className={cn(separatorVariants({ orientation }), className)} {...props} />
  );
}
