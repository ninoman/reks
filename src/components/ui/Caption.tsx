/** A small upper-case label: section and control captions (mixer, sidebars). */
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const captionVariants = cva(
  "text-2xs leading-none font-semibold tracking-caption uppercase whitespace-nowrap",
  {
    variants: {
      tone: { muted: "text-fg-muted", subtle: "text-fg-subtle", default: "text-fg" },
    },
    defaultVariants: { tone: "muted" },
  },
);

export type CaptionProps = ComponentProps<"span"> &
  VariantProps<typeof captionVariants> & { asChild?: boolean };

export function Caption({ tone, asChild = false, className, ...props }: CaptionProps) {
  const Comp = asChild ? Slot : "span";
  return <Comp className={cn(captionVariants({ tone }), className)} {...props} />;
}
