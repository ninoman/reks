/**
 * A mixer fader for one 0..1 value (Radix Slider): vertical for channel and
 * tempo faders, horizontal for the crossfader. A flat cap rides a slot.
 * Arrow keys step 1%, Page Up/Down 10% (Radix); double-click the cap
 * returns to `defaultValue`.
 */
import * as SliderPrimitive from "@radix-ui/react-slider";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { focusRing } from "./control";

export const faderVariants = cva(
  "relative flex touch-none select-none items-center justify-center data-[disabled]:opacity-45",
  {
    variants: {
      orientation: {
        vertical: "h-(--fader-len) w-(--fader-w) flex-col",
        horizontal: "h-(--fader-w) w-full",
      },
    },
    defaultVariants: { orientation: "vertical" },
  },
);

export type FaderProps = Omit<
  ComponentProps<typeof SliderPrimitive.Root>,
  "value" | "defaultValue" | "onValueChange" | "orientation" | "min" | "max" | "step"
> &
  VariantProps<typeof faderVariants> & {
    value: number;
    onValueChange: (value: number) => void;
    defaultValue?: number;
    "aria-label": string;
    /** Spoken value, e.g. "+2.5%". */
    valueText?: string;
  };

export function Fader({
  orientation,
  className,
  value,
  onValueChange,
  defaultValue,
  valueText,
  "aria-label": label,
  ...props
}: FaderProps) {
  const vertical = (orientation ?? "vertical") === "vertical";
  return (
    <SliderPrimitive.Root
      orientation={vertical ? "vertical" : "horizontal"}
      className={cn(faderVariants({ orientation }), className)}
      min={0}
      max={1}
      step={0.001}
      value={[value]}
      onValueChange={(v) => onValueChange(v[0] ?? value)}
      {...props}
    >
      <SliderPrimitive.Track
        className={cn("relative grow rounded-full bg-deck", vertical ? "w-1" : "h-1")}
      />
      <SliderPrimitive.Thumb
        aria-label={label}
        aria-valuetext={valueText}
        onDoubleClick={() => defaultValue !== undefined && onValueChange(defaultValue)}
        className={cn(
          "block rounded-xs border border-border-strong bg-knob shadow-none hover:border-fg-subtle",
          "after:absolute after:bg-fg after:content-['']",
          vertical
            ? "relative h-(--fader-cap) w-(--fader-w) after:inset-x-1 after:top-1/2 after:h-px"
            : "relative h-(--fader-w) w-(--fader-cap) after:inset-y-1 after:left-1/2 after:w-px",
          focusRing,
        )}
      />
    </SliderPrimitive.Root>
  );
}
