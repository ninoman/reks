/**
 * A small round toggle with no text (a mixer's ON lamp): dim when off, lit
 * in the accent colour when on (`aria-pressed`), with a dark inner rim in both
 * states. `aria-label` names it.
 */
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { disabledState, focusRing } from "./control";

export type LampButtonProps = Omit<ComponentProps<"button">, "aria-label" | "aria-pressed"> & {
  "aria-label": string;
  pressed: boolean;
};

export function LampButton({ pressed, className, type, ...props }: LampButtonProps) {
  return (
    <button
      type={type ?? "button"}
      aria-pressed={pressed}
      className={cn(
        "size-3.5 shrink-0 cursor-default rounded-full border border-border-strong bg-surface-strong inset-shadow-lamp",
        "transition-colors duration-(--duration-fast) hover:border-fg-subtle",
        "aria-pressed:border-accent-solid aria-pressed:bg-accent-solid aria-pressed:shadow-lit",
        focusRing,
        disabledState,
        className,
      )}
      {...props}
    />
  );
}
