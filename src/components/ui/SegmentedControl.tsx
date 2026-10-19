/**
 * One choice out of a few, as joined segments (Radix ToggleGroup, single):
 * role="radiogroup" with role="radio" items. The choice can't be cleared.
 */
import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { type ControlSize, focusRingInset } from "./control";

export const segmentedControlVariants = cva(
  "inline-flex items-stretch gap-0.5 border border-border-strong bg-surface-strong p-0.5",
  {
    variants: {
      size: {
        xs: "h-(--ctl-h-xs) rounded-(--ctl-r-xs) text-(length:--ctl-fs-xs)",
        sm: "h-(--ctl-h-sm) rounded-(--ctl-r-sm) text-(length:--ctl-fs-sm)",
        md: "h-(--ctl-h-md) rounded-(--ctl-r-md) text-(length:--ctl-fs-md)",
        lg: "h-(--ctl-h-lg) rounded-(--ctl-r-lg) text-(length:--ctl-fs-lg)",
      } satisfies Record<ControlSize, string>,
    },
    defaultVariants: { size: "md" },
  },
);

export const segmentVariants = cva([
  "inline-flex cursor-default items-center gap-1.5 rounded-[calc(var(--radius-sm)-2px)] px-(--ctl-px-sm) font-medium text-fg-muted",
  "hover:text-fg data-[state=on]:bg-surface data-[state=on]:text-fg",
  focusRingInset,
]);

export type SegmentedControlProps = Omit<
  ComponentProps<typeof ToggleGroup.Root>,
  "type" | "value" | "defaultValue" | "onValueChange"
> &
  VariantProps<typeof segmentedControlVariants> & {
    value: string;
    onValueChange: (value: string) => void;
  };

export function SegmentedControl({
  size,
  className,
  value,
  onValueChange,
  ...props
}: SegmentedControlProps) {
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={(v) => {
        if (v !== "") onValueChange(v);
      }}
      className={cn(segmentedControlVariants({ size }), className)}
      {...props}
    />
  );
}

export function Segment({ className, ...props }: ComponentProps<typeof ToggleGroup.Item>) {
  return <ToggleGroup.Item className={cn(segmentVariants(), className)} {...props} />;
}
