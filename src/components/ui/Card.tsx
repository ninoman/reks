/** A hairline-bordered group; its direct children are split by thin rules. */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const cardVariants = cva("rounded-lg border border-border", {
  variants: {
    variant: { subtle: "bg-bg-subtle", surface: "bg-surface" },
    divided: { true: "divide-y divide-border", false: "" },
  },
  defaultVariants: { variant: "subtle", divided: true },
});

export type CardProps = ComponentProps<"div"> & VariantProps<typeof cardVariants>;

export function Card({ variant, divided, className, ...props }: CardProps) {
  return <div className={cn(cardVariants({ variant, divided }), className)} {...props} />;
}
