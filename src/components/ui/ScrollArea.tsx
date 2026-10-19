/**
 * Scrolling region with thin themed scrollbars (Radix). Give it a bounded
 * height. Not for the track table: its virtualiser owns a native scroller.
 */
import * as ScrollPrimitive from "@radix-ui/react-scroll-area";
import { cva } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export const scrollAreaVariants = cva("relative overflow-hidden");

export function ScrollArea({
  className,
  viewportClassName,
  children,
  ...props
}: ComponentProps<typeof ScrollPrimitive.Root> & { viewportClassName?: string }) {
  return (
    <ScrollPrimitive.Root className={cn(scrollAreaVariants(), className)} {...props}>
      <ScrollPrimitive.Viewport
        className={cn(
          "size-full rounded-[inherit] [&>div]:block! [&>div]:min-h-full",
          viewportClassName,
        )}
      >
        {children}
      </ScrollPrimitive.Viewport>
      <ScrollPrimitive.Scrollbar
        orientation="vertical"
        className="flex w-2 touch-none select-none p-px transition-colors"
      >
        <ScrollPrimitive.Thumb className="relative flex-1 rounded-full bg-border-strong hover:bg-fg-subtle" />
      </ScrollPrimitive.Scrollbar>
      <ScrollPrimitive.Corner />
    </ScrollPrimitive.Root>
  );
}
