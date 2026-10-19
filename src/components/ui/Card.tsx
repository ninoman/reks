/** A hairline-bordered group; its direct children are split by thin rules. */
import { Slot } from "@radix-ui/react-slot";
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

export type CardProps = ComponentProps<"div"> &
  VariantProps<typeof cardVariants> & { asChild?: boolean };

export function Card({ variant, divided, asChild = false, className, ...props }: CardProps) {
  const Comp = asChild ? Slot : "div";
  return <Comp className={cn(cardVariants({ variant, divided }), className)} {...props} />;
}
