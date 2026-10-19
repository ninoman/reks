/** Styles shared by DropdownMenu and ContextMenu. */
import { cva } from "class-variance-authority";

export const menuContentVariants = cva(
  "z-(--z-menu) min-w-40 overflow-hidden rounded-md border border-border bg-surface p-1 text-sm text-fg shadow-overlay",
);

export const menuItemVariants = cva(
  [
    "relative flex h-(--menu-item-h) cursor-default select-none items-center gap-2 rounded-xs px-2 outline-none",
    "data-[highlighted]:bg-hover data-[disabled]:text-fg-subtle",
    "[&_svg]:size-(--ctl-icon-sm)",
  ],
  {
    variants: {
      variant: { default: "", danger: "text-danger" },
      inset: { true: "pl-7", false: "" },
    },
    defaultVariants: { variant: "default", inset: false },
  },
);

export const menuSeparatorClass = "-mx-1 my-1 h-px bg-border";
export const menuIndicatorClass = "absolute left-2 inline-flex items-center text-fg";
