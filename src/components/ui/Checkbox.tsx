/**
 * A native checkbox in the accent colour, sized to the control icon scale.
 * Wrap it in a <label> with its text (or give it `aria-label`).
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { disabledState, focusRing } from "./control";

export const checkboxVariants = cva(["shrink-0 accent-accent-solid", focusRing, disabledState], {
  variants: { size: { sm: "size-(--ctl-icon-sm)", md: "size-(--ctl-icon-md)" } },
  defaultVariants: { size: "sm" },
});

export type CheckboxProps = Omit<ComponentProps<"input">, "type" | "size"> &
  VariantProps<typeof checkboxVariants>;

export function Checkbox({ size, className, ...props }: CheckboxProps) {
  return <input type="checkbox" className={cn(checkboxVariants({ size }), className)} {...props} />;
}
