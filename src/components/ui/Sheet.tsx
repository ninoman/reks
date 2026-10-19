/**
 * Sheet: a panel that slides in from the right over its positioned parent, so the
 * content under it (the track table) keeps its width. Children mount when it
 * opens and unmount once the slide-out finishes, so a closed panel costs
 * nothing (no IPC).
 *
 * Driven by the Web Animations API rather than a CSS transition: it always
 * runs (no need for the closed position to be painted first), and a toggle
 * mid-slide reverses from wherever the panel is.
 */
import { cva } from "class-variance-authority";
import { type ReactNode, useLayoutEffect, useRef, useState } from "react";

import { cn } from "../../lib/cn";
import { slideIn, slideOut } from "../../lib/motion";
import { CloseIcon } from "../icons";
import { IconButton } from "./IconButton";

export const sheetVariants = cva(
  "absolute inset-y-0 right-0 z-(--z-sheet) flex w-(--sheet-w) flex-col border-l border-border bg-bg-subtle shadow-overlay",
);

const OFF = "translateX(100%)";

function reducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

export function Sheet({
  open,
  className,
  children,
}: {
  open: boolean;
  className?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  /** Still on screen: true from opening until the slide-out ends. */
  const [mounted, setMounted] = useState(false);

  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    const running = el.getAnimations?.() ?? [];
    // Mid-slide: carry on from the current position, not from an end.
    const from = running.length > 0 ? getComputedStyle(el).transform : open ? OFF : "none";
    for (const a of running) a.cancel();
    const scale = reducedMotion() ? 0 : 1;

    if (open) {
      setMounted(true);
      const timing = slideIn();
      el.animate?.([{ transform: from }, { transform: "none" }], {
        ...timing,
        duration: timing.duration * scale,
      });
      return;
    }
    if (typeof el.animate !== "function") {
      setMounted(false);
      return;
    }
    const timing = slideOut();
    const out = el.animate([{ transform: from }, { transform: OFF }], {
      ...timing,
      duration: timing.duration * scale,
      fill: "forwards",
    });
    out.onfinish = () => setMounted(false);
  }, [open]);

  if (!open && !mounted) return null;
  return (
    <div ref={ref} data-slot="sheet" className={cn(sheetVariants(), className)} inert={!open}>
      {children}
    </div>
  );
}

/** Title row with a close button. */
export function SheetHeader({
  title,
  closeLabel,
  onClose,
}: {
  title: string;
  closeLabel: string;
  onClose: () => void;
}) {
  return (
    <div className="flex h-(--ctl-h-md) shrink-0 items-center gap-1.5">
      <h2 className="m-0 flex-1 text-sm font-semibold">{title}</h2>
      <IconButton size="xs" aria-label={closeLabel} onClick={onClose}>
        <CloseIcon />
      </IconButton>
    </div>
  );
}
