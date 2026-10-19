/** A square (or round) button holding one icon; `aria-label` names it. */
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { buttonVariants } from "./Button";
import { controlSquare } from "./control";

export const iconButtonVariants = cva("", {
  variants: {
    size: controlSquare,
    shape: { square: "", round: "rounded-full" },
  },
  defaultVariants: { size: "md", shape: "square" },
});

export type IconButtonProps = Omit<ComponentProps<"button">, "aria-label"> & {
  "aria-label": string;
  asChild?: boolean;
} & VariantProps<typeof buttonVariants> &
  VariantProps<typeof iconButtonVariants>;

export function IconButton({
  variant = "ghost",
  size,
  shape,
  asChild = false,
  className,
  type,
  ...props
}: IconButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      type={asChild ? undefined : (type ?? "button")}
      className={cn(
        buttonVariants({ variant, size }),
        iconButtonVariants({ size, shape }),
        "text-fg-muted hover:text-fg aria-pressed:text-selected-fg",
        variant === "primary" && "text-accent-fg hover:text-accent-fg",
        className,
      )}
      {...props}
    />
  );
}
