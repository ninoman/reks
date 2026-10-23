/**
 * One choice out of a few, as tiles in a grid (Radix ToggleGroup, single):
 * role="radiogroup" with role="radio" tiles; the choice can't be cleared.
 * Each tile has an optional picture (`preview`, drawn on the deck panel),
 * a label and a one-line hint of its own, so long explanations sit on the
 * option they describe. The chosen tile is ringed in the accent and ticked.
 * For settings; short in-place choices stay `SegmentedControl`.
 */
import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, type ReactNode, useId } from "react";

import { cn } from "../../lib/cn";
import { CheckIcon } from "../icons";
import { Badge } from "./Badge";
import { disabledState, focusRing } from "./control";

export const choiceTilesVariants = cva("grid gap-2", {
  variants: {
    columns: { 2: "grid-cols-2", 3: "grid-cols-3" },
  },
  defaultVariants: { columns: 2 },
});

export const choiceTileVariants = cva([
  "group/tile relative flex min-w-0 cursor-default flex-col items-stretch gap-0.5 rounded-md border border-border-strong bg-surface p-2 text-left text-fg",
  "hover:border-fg-subtle data-[state=on]:border-accent data-[state=on]:bg-surface-strong data-[state=on]:ring-1 data-[state=on]:ring-accent data-[state=on]:ring-inset",
  focusRing,
  disabledState,
]);

export type ChoiceTilesProps = Omit<
  ComponentProps<typeof ToggleGroup.Root>,
  "type" | "value" | "defaultValue" | "onValueChange"
> &
  VariantProps<typeof choiceTilesVariants> & {
    value: string;
    onValueChange: (value: string) => void;
  };

export function ChoiceTiles({
  columns,
  className,
  value,
  onValueChange,
  ...props
}: ChoiceTilesProps) {
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={(v) => {
        if (v !== "") onValueChange(v);
      }}
      className={cn(choiceTilesVariants({ columns }), className)}
      {...props}
    />
  );
}

export type ChoiceTileProps = Omit<ComponentProps<typeof ToggleGroup.Item>, "children"> & {
  label: ReactNode;
  hint?: ReactNode;
  /** A small picture of the option, on the deck panel above the label. */
  preview?: ReactNode;
  /** Marks the option the app starts with. */
  isDefault?: boolean;
};

export function ChoiceTile({
  label,
  hint,
  preview,
  isDefault = false,
  className,
  ...props
}: ChoiceTileProps) {
  const id = useId();
  return (
    <ToggleGroup.Item
      aria-labelledby={`${id}-label`}
      aria-describedby={hint === undefined ? undefined : `${id}-hint`}
      className={cn(choiceTileVariants(), className)}
      {...props}
    >
      {preview !== undefined && (
        <span
          aria-hidden="true"
          className="relative mb-1.5 flex h-12 items-center justify-center overflow-hidden rounded-sm border border-border bg-deck text-deck-fg"
        >
          {preview}
        </span>
      )}
      <span
        className={cn(
          "flex flex-wrap items-center gap-1.5 text-sm font-medium",
          // Room for the tick, which sits on the preview when there is one.
          preview === undefined && "pr-5",
        )}
      >
        <span id={`${id}-label`}>{label}</span>
        {isDefault && <Badge variant="muted">Default</Badge>}
      </span>
      {hint !== undefined && (
        <span id={`${id}-hint`} className="text-xs text-fg-muted">
          {hint}
        </span>
      )}
      <span
        aria-hidden="true"
        className={cn(
          "absolute right-1.5 hidden size-4 place-items-center rounded-full bg-accent-solid text-accent-fg group-data-[state=on]/tile:grid [&_svg]:size-3",
          preview === undefined ? "top-2" : "top-1.5",
        )}
      >
        <CheckIcon />
      </span>
    </ToggleGroup.Item>
  );
}
