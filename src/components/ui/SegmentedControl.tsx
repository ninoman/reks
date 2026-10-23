/**
 * One choice out of a few, as joined segments (Radix ToggleGroup, single):
 * role="radiogroup" with role="radio" items. The choice can't be cleared.
 * `variant="control"` lights the chosen segment like a lit switch (mixer
 * assign switches); `variant="outline"` rings it in the accent colour with
 * no fill (mode tabs, e.g. the deck pad modes). Segments never break their
 * text; when they don't fit the width the row wraps instead.
 */
import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { type ControlSize, focusRingInset } from "./control";

export const segmentedControlVariants = cva(
  "group/seg inline-flex max-w-full flex-wrap items-stretch gap-0.5 border border-border-strong bg-surface-strong p-0.5",
  {
    variants: {
      // `--seg-r`: the segments' radius, the outer one less the 2px inset, so the corners nest.
      size: {
        xs: "min-h-(--ctl-h-xs) rounded-(--ctl-r-xs) text-(length:--ctl-fs-xs) [--seg-r:calc(var(--ctl-r-xs)-2px)] [--seg-h:calc(var(--ctl-h-xs)-6px)]",
        sm: "min-h-(--ctl-h-sm) rounded-(--ctl-r-sm) text-(length:--ctl-fs-sm) [--seg-r:calc(var(--ctl-r-sm)-2px)] [--seg-h:calc(var(--ctl-h-sm)-6px)]",
        md: "min-h-(--ctl-h-md) rounded-(--ctl-r-md) text-(length:--ctl-fs-md) [--seg-r:calc(var(--ctl-r-md)-2px)] [--seg-h:calc(var(--ctl-h-md)-6px)]",
        lg: "min-h-(--ctl-h-lg) rounded-(--ctl-r-lg) text-(length:--ctl-fs-lg) [--seg-r:calc(var(--ctl-r-lg)-2px)] [--seg-h:calc(var(--ctl-h-lg)-6px)]",
      } satisfies Record<ControlSize, string>,
    },
    defaultVariants: { size: "md" },
  },
);

export const segmentVariants = cva([
  "inline-flex min-h-(--seg-h) cursor-default items-center gap-1.5 whitespace-nowrap rounded-(--seg-r) px-(--ctl-px-sm) font-medium text-fg-muted",
  "hover:text-fg data-[state=on]:bg-surface data-[state=on]:text-fg",
  "group-data-[variant=control]/seg:data-[state=on]:bg-lit group-data-[variant=control]/seg:data-[state=on]:text-lit-fg group-data-[variant=control]/seg:data-[state=on]:shadow-lit",
  "group-data-[variant=outline]/seg:data-[state=on]:bg-transparent group-data-[variant=outline]/seg:data-[state=on]:text-accent group-data-[variant=outline]/seg:data-[state=on]:ring-1 group-data-[variant=outline]/seg:data-[state=on]:ring-accent group-data-[variant=outline]/seg:data-[state=on]:ring-inset",
  focusRingInset,
]);

export type SegmentedControlProps = Omit<
  ComponentProps<typeof ToggleGroup.Root>,
  "type" | "value" | "defaultValue" | "onValueChange"
> &
  VariantProps<typeof segmentedControlVariants> & {
    /** `control`: the chosen segment is lit; `outline`: ringed in the accent (read by the segments via `group/seg`). */
    variant?: "default" | "control" | "outline";
    value: string;
    onValueChange: (value: string) => void;
  };

export function SegmentedControl({
  size,
  variant = "default",
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
      data-variant={variant}
      className={cn(segmentedControlVariants({ size }), className)}
      {...props}
    />
  );
}

export function Segment({ className, ...props }: ComponentProps<typeof ToggleGroup.Item>) {
  return <ToggleGroup.Item className={cn(segmentVariants(), className)} {...props} />;
}
