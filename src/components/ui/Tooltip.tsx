/**
 * Hover/focus hint for a control (Radix). Wrap one element:
 * `<Tooltip content="Settings" shortcut="⌘ ,"><IconButton … /></Tooltip>`.
 * Rows and table cells keep the native `title` (cheaper per row).
 */
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cva } from "class-variance-authority";
import type { ReactElement, ReactNode } from "react";

import { cn } from "../../lib/cn";
import { Kbd } from "./Kbd";

/** Hover delay before a tooltip shows, ms. */
const DELAY_MS = 500;

export const tooltipVariants = cva(
  "z-(--z-tooltip) flex items-center gap-1.5 rounded-sm bg-fg px-2 py-1 text-xs text-bg shadow-overlay",
);

export function Tooltip({
  content,
  shortcut,
  side = "bottom",
  className,
  children,
}: {
  content: ReactNode;
  shortcut?: string;
  side?: "top" | "right" | "bottom" | "left";
  className?: string;
  children: ReactElement;
}) {
  return (
    // Not hoverable: a hint flipped over the controls above (a pad's, over
    // the pad mode row) closes as the pointer leaves its trigger, so it never
    // takes the click meant for what it covers.
    <TooltipPrimitive.Provider delayDuration={DELAY_MS} disableHoverableContent>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            side={side}
            sideOffset={6}
            className={cn(tooltipVariants(), className)}
          >
            {content}
            {shortcut !== undefined && <Kbd variant="inverse">{shortcut}</Kbd>}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}

/**
 * A control's own hint: wraps `children` in a tooltip when it has a keyboard
 * shortcut ("Play" + the key), and leaves it bare when it has none.
 */
export function ShortcutTooltip({
  shortcut,
  hint,
  children,
}: {
  shortcut: string | undefined;
  hint: ReactNode;
  children: ReactElement;
}) {
  if (shortcut === undefined) return children;
  return (
    <Tooltip content={hint} shortcut={shortcut}>
      {children}
    </Tooltip>
  );
}
