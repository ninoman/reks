/**
 * ResizeHandle: a drag-to-resize edge for the element it sits in (a sidebar,
 * a drawer, a table header cell). Drag it, or focus it and use the arrow
 * keys; a double-click goes back to the default width. The parent must be
 * positioned. Reports widths only: the caller stores and applies them.
 */
import { cva } from "class-variance-authority";
import { type KeyboardEvent, type PointerEvent, useRef, useState } from "react";

import { cn } from "../../lib/cn";
import { focusRingInset } from "./control";

export const resizeHandleVariants = cva(
  [
    "absolute inset-y-0 z-10 w-(--resize-handle-w) cursor-col-resize touch-none",
    // A hairline in the accent while hovered, focused or dragged.
    "after:absolute after:inset-y-0 after:w-px after:bg-accent after:opacity-0",
    "hover:after:opacity-100 focus-visible:after:opacity-100 data-[dragging]:after:opacity-100",
    focusRingInset,
  ],
  {
    variants: {
      /** The parent's edge it resizes: `right` grows rightwards, `left` leftwards. */
      edge: {
        right: "right-0 after:right-0",
        left: "left-0 after:left-0",
      },
    },
    defaultVariants: { edge: "right" },
  },
);

/** Arrow-key step, px. */
const STEP = 16;

export function ResizeHandle({
  label,
  edge = "right",
  min,
  max,
  value,
  onResize,
  onReset,
  className,
}: {
  label: string;
  edge?: "left" | "right";
  min: number;
  max: number;
  /** The current width, when the caller knows it (else the parent is measured). */
  value?: number;
  onResize: (width: number) => void;
  onReset: () => void;
  className?: string;
}) {
  const drag = useRef<{ x: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const sign = edge === "right" ? 1 : -1;
  const clamp = (w: number) => Math.round(Math.min(max, Math.max(min, w)));
  const current = (el: HTMLElement) =>
    value ?? el.parentElement?.getBoundingClientRect().width ?? min;

  function onPointerDown(e: PointerEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    // Not a sort click, row selection or text selection.
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, width: current(e.currentTarget) };
    setDragging(true);
  }

  function onPointerMove(e: PointerEvent<HTMLDivElement>) {
    const d = drag.current;
    if (d !== null) onResize(clamp(d.width + sign * (e.clientX - d.x)));
  }

  function onPointerEnd() {
    drag.current = null;
    setDragging(false);
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const delta = { ArrowRight: STEP, ArrowLeft: -STEP }[e.key];
    if (delta === undefined) return;
    e.preventDefault();
    e.stopPropagation();
    onResize(clamp(current(e.currentTarget) + sign * delta));
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable splitter has no native element.
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      tabIndex={0}
      title="Drag to resize, double-click to reset"
      data-dragging={dragging || undefined}
      className={cn(resizeHandleVariants({ edge }), className)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerEnd}
      onPointerCancel={onPointerEnd}
      onKeyDown={onKeyDown}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => {
        e.stopPropagation();
        onReset();
      }}
    />
  );
}
