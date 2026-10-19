/**
 * A label with its control, plus an optional hint or error underneath.
 * `vertical` and `horizontal` wrap the control in a <label>, so it takes the
 * label's text as its name. `inline` (filter bar) is a plain row; its
 * control names itself, and `active` outlines it while it filters.
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ReactNode } from "react";

import { cn } from "../../lib/cn";

export const fieldVariants = cva("flex min-w-0", {
  variants: {
    orientation: {
      vertical: "flex-col gap-1",
      horizontal: "items-center justify-between gap-4",
      inline: "items-center gap-1.5 rounded-(--ctl-r-sm) border border-transparent px-1 py-px",
    },
    active: { true: "", false: "" },
  },
  compoundVariants: [{ orientation: "inline", active: true, class: "border-accent" }],
  defaultVariants: { orientation: "vertical", active: false },
});

export const fieldLabelVariants = cva("", {
  variants: {
    orientation: {
      vertical: "text-xs text-fg-muted",
      horizontal: "text-sm font-semibold text-fg",
      inline: "text-xs text-fg-muted",
    },
  },
  defaultVariants: { orientation: "vertical" },
});

export function Field({
  label,
  hint,
  error,
  orientation = "vertical",
  active = false,
  className,
  children,
}: VariantProps<typeof fieldVariants> & {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  const labelText = (
    <span data-slot="field-label" className={fieldLabelVariants({ orientation })}>
      {label}
    </span>
  );
  const body =
    orientation === "inline" ? (
      <div
        data-slot="field"
        data-active={active || undefined}
        className={cn(fieldVariants({ orientation, active }), className)}
      >
        {labelText}
        {children}
      </div>
    ) : (
      // biome-ignore lint/a11y/noLabelWithoutControl: the control is `children`.
      <label
        data-slot="field"
        className={cn(fieldVariants({ orientation, active }), hint || error ? "" : className)}
      >
        {labelText}
        {children}
      </label>
    );
  if (!hint && !error) return body;
  return (
    <div className={cn("flex min-w-0 flex-col gap-1", className)}>
      {body}
      {hint && <p className="m-0 text-xs text-fg-muted">{hint}</p>}
      {error && (
        <p className="m-0 text-xs text-danger" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
