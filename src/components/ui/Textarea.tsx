/** Multi-line text field; same surface and focus as `Input`. */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { disabledState, fieldSurface, focusRing } from "./control";

export const textareaVariants = cva(
  ["w-full resize-y select-text py-1", fieldSurface, focusRing, disabledState],
  {
    variants: {
      size: {
        sm: "rounded-(--ctl-r-sm) px-(--ctl-px-sm) text-(length:--ctl-fs-sm)",
        md: "rounded-(--ctl-r-md) px-(--ctl-px-md) text-(length:--ctl-fs-md)",
      },
    },
    defaultVariants: { size: "md" },
  },
);

export type TextareaProps = ComponentProps<"textarea"> & VariantProps<typeof textareaVariants>;

export function Textarea({ size, className, ...props }: TextareaProps) {
  return <textarea className={cn(textareaVariants({ size }), className)} {...props} />;
}
