/** On/off switch (Radix): role="switch" with `aria-checked`. */
import * as SwitchPrimitive from "@radix-ui/react-switch";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { disabledState, focusRing } from "./control";

export const switchVariants = cva(
  [
    "inline-flex shrink-0 cursor-default items-center rounded-full bg-border-strong p-0.5",
    "transition-colors duration-(--duration-fast) data-[state=checked]:bg-accent-solid",
    focusRing,
    disabledState,
  ],
  {
    variants: { size: { sm: "h-3.5 w-6.5", md: "h-4.5 w-8" } },
    defaultVariants: { size: "sm" },
  },
);

const thumbVariants = cva(
  // On, the thumb takes the filled track's text colour: a light thumb vanishes on Booth's light accent.
  "block rounded-full bg-knob transition-transform duration-(--duration-fast) data-[state=checked]:bg-accent-fg",
  {
    variants: {
      size: {
        sm: "size-2.5 data-[state=checked]:translate-x-3",
        md: "size-3.5 data-[state=checked]:translate-x-3.5",
      },
    },
    defaultVariants: { size: "sm" },
  },
);

export type SwitchProps = ComponentProps<typeof SwitchPrimitive.Root> &
  VariantProps<typeof switchVariants>;

export function Switch({ size, className, ...props }: SwitchProps) {
  return (
    <SwitchPrimitive.Root className={cn(switchVariants({ size }), className)} {...props}>
      <SwitchPrimitive.Thumb className={thumbVariants({ size })} />
    </SwitchPrimitive.Root>
  );
}
