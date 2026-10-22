/**
 * Performance pads (decks): `Pad`, one button, and `SplitPad`, two half
 * buttons (back / forward) under one label. `tone`: `set` (filled, in
 * `color` when given: a data colour such as a hot cue's, like `Swatch`;
 * else `deck-hotcue`),
 * `empty` (dashed), `lit` (the active one). `marked` is a heavy outline
 * (the size used last). They fill their grid cell, at least `--pad-h` tall.
 * A `Pad` with a `hint` or `shortcut` shows them in a tooltip.
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps, CSSProperties, ReactNode } from "react";
import { cn } from "../../lib/cn";
import { darkText } from "../../lib/deckMarks";
import { disabledState, focusRing } from "./control";
import { ShortcutTooltip, Tooltip } from "./Tooltip";

export const padVariants = cva(
  [
    "relative flex min-h-(--pad-h) min-w-0 cursor-default items-center gap-1 overflow-hidden whitespace-nowrap rounded-xs border px-1.5 text-2xs font-semibold",
    "transition-colors duration-(--duration-fast)",
    focusRing,
    disabledState,
  ],
  {
    variants: {
      tone: {
        default:
          "justify-center border-border-strong bg-surface-strong text-fg hover:border-fg-subtle active:brightness-110",
        // Without `color`: the deck's hot cue colour.
        set: "border-(color:--pad-color,var(--color-deck-hotcue)) bg-(--pad-color,var(--color-deck-hotcue)) text-(color:--pad-fg,var(--color-pad-ink-dark)) hover:brightness-110 active:brightness-95",
        empty: "border-dashed border-border-strong bg-transparent text-fg-subtle",
        lit: "justify-center border-lit bg-lit text-lit-fg shadow-lit",
      },
      marked: { true: "border-2 border-fg", false: "" },
    },
    compoundVariants: [{ tone: "lit", marked: true, class: "border-lit" }],
    defaultVariants: { tone: "default", marked: false },
  },
);

type PadStyle = VariantProps<typeof padVariants> & {
  /** `#rrggbb` fill for `tone="set"`; text turns dark or light to stay readable. */
  color?: string | undefined;
};

function padStyle(color: string | undefined): CSSProperties | undefined {
  if (color === undefined) return undefined;
  return {
    "--pad-color": color,
    "--pad-fg": darkText(color) ? "var(--color-pad-ink-dark)" : "var(--color-pad-ink-light)",
  } as CSSProperties;
}

export type PadProps = ComponentProps<"button"> &
  PadStyle & {
    "aria-label": string;
    shortcut?: string | undefined;
    hint?: ReactNode;
  };

export function Pad({ tone, marked, color, className, style, shortcut, hint, ...props }: PadProps) {
  const button = (
    <button
      type="button"
      className={cn(padVariants({ tone, marked }), className)}
      style={{ ...padStyle(color), ...style }}
      {...props}
    />
  );
  if (shortcut === undefined && hint === undefined) return button;
  return (
    <Tooltip
      content={hint ?? props["aria-label"]}
      {...(shortcut === undefined ? {} : { shortcut })}
    >
      {button}
    </Tooltip>
  );
}

export interface SplitPadProps extends PadStyle {
  /** Shared label in the middle, e.g. the jump size. */
  label: ReactNode;
  backLabel: string;
  forwardLabel: string;
  backShortcut?: string | undefined;
  forwardShortcut?: string | undefined;
  onBack: () => void;
  onForward: () => void;
  disabled?: boolean;
  className?: string;
}

const half =
  "flex h-full items-center px-1 text-fg-muted hover:bg-hover active:bg-surface-strong disabled:hover:bg-transparent";

export function SplitPad({
  label,
  backLabel,
  forwardLabel,
  backShortcut,
  forwardShortcut,
  onBack,
  onForward,
  disabled,
  tone,
  marked,
  color,
  className,
}: SplitPadProps) {
  return (
    <div
      className={cn(padVariants({ tone, marked }), "grid grid-cols-2 gap-0 p-0", className)}
      style={padStyle(color)}
      data-disabled={disabled || undefined}
    >
      <ShortcutTooltip shortcut={backShortcut} hint={backLabel}>
        <button
          type="button"
          aria-label={backLabel}
          disabled={disabled}
          onClick={onBack}
          className={cn(half, "justify-start", focusRing, disabledState)}
        >
          ◀
        </button>
      </ShortcutTooltip>
      <ShortcutTooltip shortcut={forwardShortcut} hint={forwardLabel}>
        <button
          type="button"
          aria-label={forwardLabel}
          disabled={disabled}
          onClick={onForward}
          className={cn(half, "justify-end", focusRing, disabledState)}
        >
          ▶
        </button>
      </ShortcutTooltip>
      <span
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 font-mono data-[disabled]:opacity-45"
        data-disabled={disabled || undefined}
      >
        {label}
      </span>
    </div>
  );
}
