/**
 * A native checkbox in the accent colour, sized to the control icon scale.
 * Wrap it in a <label> with its text (or give it `aria-label`).
 */
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, useEffect, useRef } from "react";

import { cn } from "../../lib/cn";
import { disabledState, focusRing } from "./control";

export const checkboxVariants = cva(["shrink-0 accent-accent-solid", focusRing, disabledState], {
  variants: { size: { sm: "size-(--ctl-icon-sm)", md: "size-(--ctl-icon-md)" } },
  defaultVariants: { size: "sm" },
});

export type CheckboxProps = Omit<ComponentProps<"input">, "type" | "size"> &
  VariantProps<typeof checkboxVariants> & {
    /** Mixed: some of what the box stands for is on. A click still toggles `checked`. */
    indeterminate?: boolean;
  };

export function Checkbox({ size, className, indeterminate = false, ref, ...props }: CheckboxProps) {
  const own = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (own.current !== null) own.current.indeterminate = indeterminate;
  }, [indeterminate]);
  return (
    <input
      type="checkbox"
      className={cn(checkboxVariants({ size }), className)}
      ref={(el) => {
        own.current = el;
        if (typeof ref === "function") ref(el);
        else if (ref) ref.current = el;
      }}
      {...props}
    />
  );
}
