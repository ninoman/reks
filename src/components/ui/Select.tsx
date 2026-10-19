/**
 * A native <select> with our surface and chevron: the OS menu and keyboard
 * stay as they are. `className` sizes the wrapper.
 */
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { ChevronDownIcon } from "../icons";
import { controlSizes, disabledState, fieldSurface, focusRing } from "./control";

export const selectVariants = cva(
  [
    "w-full appearance-none pr-[calc(var(--ctl-icon-md)+6px)]",
    fieldSurface,
    focusRing,
    disabledState,
  ],
  {
    variants: { size: controlSizes },
    defaultVariants: { size: "md" },
  },
);

export type SelectProps = Omit<ComponentProps<"select">, "size"> &
  VariantProps<typeof selectVariants>;

export function Select({ size, className, children, ...props }: SelectProps) {
  return (
    <span className={cn("relative inline-flex min-w-0", className)}>
      <select className={selectVariants({ size })} {...props}>
        {children}
      </select>
      <span className="pointer-events-none absolute inset-y-0 right-1.5 flex items-center text-fg-muted [&_svg]:size-(--ctl-icon-sm)">
        <ChevronDownIcon />
      </span>
    </span>
  );
}
