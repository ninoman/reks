/** A single-value slider (Radix); `aria-label` names its thumb. */
import * as SliderPrimitive from "@radix-ui/react-slider";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { focusRing } from "./control";

export const sliderVariants = cva(
  "relative flex touch-none select-none items-center data-[disabled]:opacity-45",
  {
    variants: { size: { sm: "h-(--ctl-h-sm)", md: "h-(--ctl-h-md)" } },
    defaultVariants: { size: "sm" },
  },
);

export type SliderProps = Omit<
  ComponentProps<typeof SliderPrimitive.Root>,
  "value" | "defaultValue" | "onValueChange"
> &
  VariantProps<typeof sliderVariants> & {
    value: number;
    onValueChange: (value: number) => void;
    "aria-label": string;
  };

export function Slider({
  size,
  className,
  value,
  onValueChange,
  "aria-label": label,
  ...props
}: SliderProps) {
  return (
    <SliderPrimitive.Root
      className={cn(sliderVariants({ size }), className)}
      value={[value]}
      onValueChange={(v) => onValueChange(v[0] ?? value)}
      {...props}
    >
      <SliderPrimitive.Track className="relative h-1 grow overflow-hidden rounded-full bg-border-strong">
        <SliderPrimitive.Range className="absolute h-full bg-fg-muted" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb
        aria-label={label}
        className={cn(
          "block size-3 rounded-full border border-border-strong bg-knob hover:border-fg-subtle",
          focusRing,
        )}
      />
    </SliderPrimitive.Root>
  );
}
