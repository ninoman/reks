/**
 * Button. `aria-pressed` turns it into a toggle (the selected tint), so the
 * toolbar's on/off buttons need no extra component.
 */
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { controlSizes, disabledState, focusRing } from "./control";

export const buttonVariants = cva(
  [
    "inline-flex shrink-0 cursor-default items-center justify-center whitespace-nowrap border border-transparent bg-transparent font-medium",
    "transition-colors duration-(--duration-fast)",
    "aria-pressed:border-border-strong aria-pressed:bg-selected aria-pressed:text-selected-fg",
    focusRing,
    disabledState,
  ],
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-fg hover:bg-accent-hover active:bg-accent-active",
        secondary:
          "border-border-strong bg-surface text-fg hover:border-fg-subtle active:bg-surface-strong",
        outline: "border-border-strong text-fg hover:bg-hover active:bg-surface-strong",
        ghost: "text-fg hover:bg-hover active:bg-surface-strong",
        danger: "bg-danger text-danger-fg hover:brightness-110 active:brightness-95",
        /** Quiet text action; turns to danger on hover (e.g. "Remove"). */
        quiet: "px-1 text-fg-muted hover:text-danger",
        link: "px-0 text-fg-muted underline-offset-2 hover:text-fg hover:underline",
      },
      size: controlSizes,
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & { asChild?: boolean };

export function Button({ variant, size, asChild = false, className, type, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      type={asChild ? undefined : (type ?? "button")}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}
