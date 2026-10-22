/**
 * A mixer fader for one 0..1 value (Radix Slider): vertical for channel and
 * tempo faders, horizontal for the crossfader. A flat cap rides a slot.
 * Arrow keys step 1%, Page Up/Down 10% (Radix); double-click the cap or
 * Delete returns to `defaultValue`. `detent` draws a tick at that value and
 * pulls drags within `--fader-detent` of it onto it (keys step past it).
 */
import * as SliderPrimitive from "@radix-ui/react-slider";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, useRef } from "react";

import { cn } from "../../lib/cn";
import { snapToDetent } from "../../lib/mixView";
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
    /** 0..1: a tick here that pointer drags snap to. */
    detent?: number;
  };

/** The `--fader-detent` zone as a fraction of the fader's travel. */
function detentZone(el: HTMLElement, vertical: boolean): number {
  const px = Number.parseFloat(getComputedStyle(el).getPropertyValue("--fader-detent")) || 0;
  const len = vertical ? el.clientHeight : el.clientWidth;
  return len > 0 ? px / len : 0;
}

export function Fader({
  orientation,
  className,
  value,
  onValueChange,
  defaultValue,
  valueText,
  detent,
  "aria-label": label,
  ...props
}: FaderProps) {
  const vertical = (orientation ?? "vertical") === "vertical";
  const root = useRef<HTMLSpanElement>(null);
  // Only drags snap: arrow keys must be able to step off the detent.
  const dragging = useRef(false);
  function change(v: number) {
    const el = root.current;
    if (detent === undefined || !dragging.current || el === null) return onValueChange(v);
    onValueChange(snapToDetent(v, detent, detentZone(el, vertical)));
  }
  return (
    <SliderPrimitive.Root
      ref={root}
      orientation={vertical ? "vertical" : "horizontal"}
      className={cn(faderVariants({ orientation }), className)}
      min={0}
      max={1}
      step={0.001}
      value={[value]}
      onValueChange={(v) => change(v[0] ?? value)}
      onPointerDown={() => {
        dragging.current = true;
      }}
      onPointerUp={() => {
        dragging.current = false;
      }}
      onLostPointerCapture={() => {
        dragging.current = false;
      }}
      {...props}
    >
      <SliderPrimitive.Track
        className={cn("relative grow rounded-full bg-deck", vertical ? "w-1" : "h-1")}
      />
      {detent !== undefined && (
        <span
          aria-hidden="true"
          data-detent=""
          className={cn(
            "pointer-events-none absolute bg-fg-muted",
            vertical ? "inset-x-0 h-px" : "inset-y-0 w-px",
          )}
          // Radix puts 0 at the bottom of a vertical slider.
          style={vertical ? { bottom: `${detent * 100}%` } : { left: `${detent * 100}%` }}
        />
      )}
      <SliderPrimitive.Thumb
        aria-label={label}
        aria-valuetext={valueText}
        onDoubleClick={() => defaultValue !== undefined && onValueChange(defaultValue)}
        onKeyDown={(e) => {
          if ((e.key === "Delete" || e.key === "Backspace") && defaultValue !== undefined) {
            e.preventDefault();
            onValueChange(defaultValue);
          }
        }}
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
