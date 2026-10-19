/** Floating panel opened from a trigger (Radix). Content portals to <body>. */
import * as PopoverPrimitive from "@radix-ui/react-popover";
import { cva } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const popoverContentVariants = cva(
  "z-(--z-popover) rounded-md border border-border bg-surface p-1.5 text-fg shadow-overlay outline-none",
);

export const Popover = PopoverPrimitive.Root;
export const PopoverTrigger = PopoverPrimitive.Trigger;

export function PopoverContent({
  className,
  sideOffset = 4,
  align = "start",
  ...props
}: ComponentProps<typeof PopoverPrimitive.Content>) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        sideOffset={sideOffset}
        align={align}
        className={cn(popoverContentVariants(), className)}
        {...props}
      />
    </PopoverPrimitive.Portal>
  );
}
