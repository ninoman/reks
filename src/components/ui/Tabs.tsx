/**
 * Tabs (Radix): a list of triggers and one panel per tab; arrow keys move
 * between triggers. `orientation="vertical"` stacks the list as a sidebar
 * (Settings); the chosen tab takes the neutral `selected` tint.
 */
import * as TabsPrimitive from "@radix-ui/react-tabs";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { controlSizes, focusRingInset, hoverState } from "./control";

export const Tabs = TabsPrimitive.Root;

export function TabsList({ className, ...props }: ComponentProps<typeof TabsPrimitive.List>) {
  return (
    <TabsPrimitive.List
      className={cn("flex gap-0.5 data-[orientation=vertical]:flex-col", className)}
      {...props}
    />
  );
}

export function TabsTrigger({ className, ...props }: ComponentProps<typeof TabsPrimitive.Trigger>) {
  return (
    <TabsPrimitive.Trigger
      className={cn(
        controlSizes.md,
        "inline-flex cursor-default items-center font-medium text-fg-muted",
        hoverState,
        "hover:text-fg data-[state=active]:bg-selected data-[state=active]:text-selected-fg",
        focusRingInset,
        className,
      )}
      {...props}
    />
  );
}

export function TabsContent({ className, ...props }: ComponentProps<typeof TabsPrimitive.Content>) {
  return <TabsPrimitive.Content className={cn("outline-none", className)} {...props} />;
}
