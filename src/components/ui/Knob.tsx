/**
 * A rotary knob for one 0..1 value (role="slider"; Radix has no knob). Drag
 * up or down (Shift: fine), arrow keys step 1% (Shift: 0.2%), Page Up/Down
 * 10%, Home/End the ends; double-click or Delete returns to `defaultValue`.
 * `bipolar` draws the arc from the centre (EQ, trim). `lit` lights it like a
 * switch that is on (a channel knob off centre); `detent` holds drags within
 * `DETENT_ZONE` of that value on it (keys step past it).
 */
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, type KeyboardEvent, type PointerEvent, useRef } from "react";

import { cn } from "../../lib/cn";
import { snapToDetent } from "../../lib/mixView";
import { disabledState, focusRing } from "./control";

export const knobVariants = cva(
  [
    "relative inline-block shrink-0 cursor-ns-resize touch-none rounded-full",
    focusRing,
    disabledState,
  ],
  {
    variants: {
      size: {
        sm: "size-(--knob-sm)",
        md: "size-(--knob-md)",
        lg: "size-(--knob-lg)",
      },
    },
    defaultVariants: { size: "md" },
  },
);

/** Travel of the pointer, in px, for the whole range. */
const DRAG_PX = 160;
/** A drag within this much of the travel from `detent` sticks there. */
export const DETENT_ZONE = 0.03;
/** Sweep of the knob: 270°, from 7:30 to 4:30. */
const SWEEP = 270;

/** Radii of the position mark's ends, in the 0..32 box. The cap is 10.5; the mark pokes past it, short of the arc at 14. */
const MARK_INNER = 3;
const MARK_OUTER = 12.5;

const clamp = (v: number) => Math.min(1, Math.max(0, v));

/** The value a key press moves to, or null for keys the knob ignores. */
export function knobKeyValue(
  key: string,
  value: number,
  fine: boolean,
  reset: number,
): number | null {
  const step = fine ? 0.002 : 0.01;
  switch (key) {
    case "ArrowUp":
    case "ArrowRight":
      return clamp(value + step);
    case "ArrowDown":
    case "ArrowLeft":
      return clamp(value - step);
    case "PageUp":
      return clamp(value + 0.1);
    case "PageDown":
      return clamp(value - 0.1);
    case "Home":
      return 0;
    case "End":
      return 1;
    case "Delete":
    case "Backspace":
      return reset;
    default:
      return null;
  }
}

/**
 * The value a drag moves to: `dy` px up from the press at `from`, five times
 * finer with `fine`; within `DETENT_ZONE` of `detent` it sticks there.
 */
export function knobDragValue(from: number, dy: number, fine: boolean, detent?: number): number {
  const next = clamp(from + dy / (DRAG_PX * (fine ? 5 : 1)));
  return detent === undefined ? next : snapToDetent(next, detent, DETENT_ZONE);
}

/** A point on the knob's circle at `v` (0..1), in a 0..32 box. */
function point(v: number, r: number): [number, number] {
  const a = ((-135 + v * SWEEP - 90) * Math.PI) / 180;
  return [16 + r * Math.cos(a), 16 + r * Math.sin(a)];
}

/** The position mark: a short line from near the centre out past the cap's rim. */
export function knobMark(v: number): { x1: number; y1: number; x2: number; y2: number } {
  const [x1, y1] = point(v, MARK_INNER);
  const [x2, y2] = point(v, MARK_OUTER);
  return { x1, y1, x2, y2 };
}

function arc(from: number, to: number, r: number): string {
  const [x0, y0] = point(from, r);
  const [x1, y1] = point(to, r);
  const large = Math.abs(to - from) * SWEEP > 180 ? 1 : 0;
  const sweep = to > from ? 1 : 0;
  return `M ${x0} ${y0} A ${r} ${r} 0 ${large} ${sweep} ${x1} ${y1}`;
}

export type KnobProps = Omit<ComponentProps<"div">, "onChange" | "defaultValue"> &
  VariantProps<typeof knobVariants> & {
    value: number;
    onValueChange: (value: number) => void;
    /** Where double-click and Delete return to. */
    defaultValue?: number;
    bipolar?: boolean;
    /** Lit like a switch that is on. */
    lit?: boolean;
    /** A value that drags stick to, within `DETENT_ZONE`. */
    detent?: number;
    disabled?: boolean;
    "aria-label": string;
    /** Spoken value, e.g. "+3 dB"; defaults to a percentage. */
    valueText?: string;
  };

export function Knob({
  size,
  className,
  value,
  onValueChange,
  defaultValue = 0,
  bipolar = false,
  lit = false,
  detent,
  disabled = false,
  valueText,
  ...props
}: KnobProps) {
  const drag = useRef<{ y: number; value: number } | null>(null);
  const v = clamp(value);
  const set = (next: number) => {
    if (next !== value) onValueChange(next);
  };

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (disabled) return;
    const next = knobKeyValue(e.key, v, e.shiftKey, defaultValue);
    if (next === null) return;
    e.preventDefault();
    set(next);
  }

  function onPointerDown(e: PointerEvent<HTMLDivElement>) {
    if (disabled || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    e.currentTarget.focus();
    drag.current = { y: e.clientY, value: v };
  }

  function onPointerMove(e: PointerEvent<HTMLDivElement>) {
    const d = drag.current;
    if (d === null) return;
    set(knobDragValue(d.value, d.y - e.clientY, e.shiftKey, detent));
  }

  function onPointerUp() {
    drag.current = null;
  }

  const start = bipolar ? 0.5 : 0;
  const mark = knobMark(v);
  return (
    <div
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      aria-valuetext={valueText}
      aria-disabled={disabled || undefined}
      data-lit={lit || undefined}
      className={cn(knobVariants({ size }), lit && "shadow-lit", className)}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={() => !disabled && set(defaultValue)}
      {...props}
    >
      <svg viewBox="0 0 32 32" className="size-full" aria-hidden="true">
        <path
          d={arc(0, 1, 14)}
          fill="none"
          strokeWidth="2.5"
          className="stroke-border-strong"
          strokeLinecap="round"
        />
        {Math.abs(v - start) > 0.004 && (
          <path
            d={arc(Math.min(start, v), Math.max(start, v), 14)}
            fill="none"
            strokeWidth={lit ? 3 : 2.5}
            className={lit ? "stroke-lit" : "stroke-fg-muted"}
            strokeLinecap="round"
          />
        )}
        <circle
          cx="16"
          cy="16"
          r="10.5"
          className={lit ? "fill-lit stroke-lit" : "fill-knob stroke-border-strong"}
          strokeWidth="1"
        />
        <line
          {...mark}
          strokeWidth="2"
          strokeLinecap="round"
          className={lit ? "stroke-lit-fg" : "stroke-knob-mark"}
        />
      </svg>
    </div>
  );
}
