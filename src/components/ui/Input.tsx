/**
 * Text input. `variant="search"` adds the leading search icon; `className`
 * then sizes the wrapper, so both variants are placed the same way.
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { SearchIcon } from "../icons";
import { controlSizes, disabledState, fieldSurface, focusRing } from "./control";

export const inputVariants = cva(["w-full select-text", fieldSurface, focusRing, disabledState], {
  variants: {
    variant: { default: "", search: "pl-[calc(var(--ctl-icon-md)+10px)]" },
    size: controlSizes,
  },
  defaultVariants: { variant: "default", size: "md" },
});

export type InputProps = Omit<ComponentProps<"input">, "size"> & VariantProps<typeof inputVariants>;

export function Input({ variant, size, className, ...props }: InputProps) {
  if (variant !== "search") {
    return <input className={cn(inputVariants({ variant, size }), className)} {...props} />;
  }
  return (
    <span className={cn("relative inline-flex min-w-0", className)}>
      <span className="pointer-events-none absolute inset-y-0 left-2 flex items-center text-fg-muted [&_svg]:size-(--ctl-icon-sm)">
        <SearchIcon />
      </span>
      <input type="search" className={inputVariants({ variant, size })} {...props} />
    </span>
  );
}
