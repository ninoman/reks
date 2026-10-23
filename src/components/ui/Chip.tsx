/** A label pill (a tag on a track, a notice). `onRemove` adds the × button. */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { CloseIcon } from "../icons";
import { focusRing } from "./control";

export const chipVariants = cva(
  "inline-flex max-w-full items-center gap-1 rounded-full border bg-surface-strong px-2 text-xs leading-5 text-fg",
  {
    variants: {
      // `notice`: something off screen needs a look (a deck playing unseen).
      tone: { neutral: "border-border", notice: "border-warning" },
    },
    defaultVariants: { tone: "neutral" },
  },
);

export type ChipProps = ComponentProps<"span"> &
  VariantProps<typeof chipVariants> & {
    onRemove?: () => void;
    /** Makes the label a button (e.g. "Filter by this tag"); `activateLabel` names it. */
    onActivate?: () => void;
    activateLabel?: string;
    /** Names the × button: "Remove {removeLabel}". */
    removeLabel?: string;
  };

export function Chip({
  className,
  tone,
  onRemove,
  removeLabel,
  onActivate,
  activateLabel,
  children,
  ...props
}: ChipProps) {
  return (
    <span className={cn(chipVariants({ tone }), className)} {...props}>
      {onActivate === undefined ? (
        <span className="truncate">{children}</span>
      ) : (
        <button
          type="button"
          title={activateLabel}
          aria-label={activateLabel}
          className={cn("min-w-0 cursor-pointer truncate rounded-full hover:underline", focusRing)}
          onClick={onActivate}
        >
          {children}
        </button>
      )}
      {onRemove !== undefined && (
        <button
          type="button"
          aria-label={`Remove ${removeLabel ?? "tag"}`}
          className={cn(
            "-mr-1 inline-flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-full text-fg-muted hover:bg-hover hover:text-fg [&_svg]:size-3",
            focusRing,
          )}
          onClick={onRemove}
        >
          <CloseIcon />
        </button>
      )}
    </span>
  );
}
