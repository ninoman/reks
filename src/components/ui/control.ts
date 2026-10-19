/**
 * Shared pieces of every interactive control: the size scale (height,
 * padding, font, icon, radius and gap from `--ctl-*` in tokens.css) and the
 * focus, disabled and hover states. Controls of one size are one height.
 */

export type ControlSize = "xs" | "sm" | "md" | "lg";

/** Height + padding + font + radius + gap + icon size, per size. */
export const controlSizes: Record<ControlSize, string> = {
  xs: "h-(--ctl-h-xs) px-(--ctl-px-xs) text-(length:--ctl-fs-xs) rounded-(--ctl-r-xs) gap-(--ctl-gap-xs) [&_svg]:size-(--ctl-icon-xs)",
  sm: "h-(--ctl-h-sm) px-(--ctl-px-sm) text-(length:--ctl-fs-sm) rounded-(--ctl-r-sm) gap-(--ctl-gap-sm) [&_svg]:size-(--ctl-icon-sm)",
  md: "h-(--ctl-h-md) px-(--ctl-px-md) text-(length:--ctl-fs-md) rounded-(--ctl-r-md) gap-(--ctl-gap-md) [&_svg]:size-(--ctl-icon-md)",
  lg: "h-(--ctl-h-lg) px-(--ctl-px-lg) text-(length:--ctl-fs-lg) rounded-(--ctl-r-lg) gap-(--ctl-gap-lg) [&_svg]:size-(--ctl-icon-lg)",
};

/** Square controls (icon buttons): width = height. */
export const controlSquare: Record<ControlSize, string> = {
  xs: "w-(--ctl-h-xs) px-0",
  sm: "w-(--ctl-h-sm) px-0",
  md: "w-(--ctl-h-md) px-0",
  lg: "w-(--ctl-h-lg) px-0",
};

/** Keyboard focus: the accent ring, outside the control. */
export const focusRing =
  "outline-hidden focus-visible:outline-solid focus-visible:outline-(length:--focus-ring-width) focus-visible:outline-offset-1 focus-visible:outline-focus-ring";

/** Same ring, inset, for things that fill their parent (rows, cells). */
export const focusRingInset =
  "outline-hidden focus-visible:outline-solid focus-visible:outline-(length:--focus-ring-width) focus-visible:-outline-offset-2 focus-visible:outline-focus-ring";

export const disabledState =
  "disabled:cursor-default disabled:opacity-45 aria-disabled:cursor-default aria-disabled:opacity-45";

/** Neutral hover / press for quiet surfaces (ghost buttons, rows, menu items). */
export const hoverState = "hover:bg-hover active:bg-surface-strong";

/** Text fields: shared by Input, Textarea and Select. */
export const fieldSurface =
  "min-w-0 border border-border-strong bg-surface text-fg placeholder:text-fg-subtle hover:border-fg-subtle focus-visible:border-accent";
