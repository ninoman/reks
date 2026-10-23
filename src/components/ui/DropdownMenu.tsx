/** Button-opened menu (Radix). Content portals to <body>. */
import * as Menu from "@radix-ui/react-dropdown-menu";
import type { VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { CheckIcon } from "../icons";
import {
  menuContentVariants,
  menuIndicatorClass,
  menuItemVariants,
  menuSeparatorClass,
} from "./menu";

export { menuContentVariants as dropdownMenuContentVariants };
export const DropdownMenu = Menu.Root;
export const DropdownMenuTrigger = Menu.Trigger;

export function DropdownMenuContent({
  className,
  sideOffset = 4,
  align = "end",
  ...props
}: ComponentProps<typeof Menu.Content>) {
  return (
    <Menu.Portal>
      <Menu.Content
        sideOffset={sideOffset}
        align={align}
        className={cn(menuContentVariants(), className)}
        {...props}
      />
    </Menu.Portal>
  );
}

export function DropdownMenuItem({
  className,
  variant,
  inset,
  ...props
}: ComponentProps<typeof Menu.Item> & VariantProps<typeof menuItemVariants>) {
  return <Menu.Item className={cn(menuItemVariants({ variant, inset }), className)} {...props} />;
}

export function DropdownMenuCheckboxItem({
  className,
  children,
  ...props
}: ComponentProps<typeof Menu.CheckboxItem>) {
  return (
    <Menu.CheckboxItem className={cn(menuItemVariants({ inset: true }), className)} {...props}>
      <Menu.ItemIndicator className={menuIndicatorClass}>
        <CheckIcon />
      </Menu.ItemIndicator>
      {children}
    </Menu.CheckboxItem>
  );
}

export function DropdownMenuSeparator({
  className,
  ...props
}: ComponentProps<typeof Menu.Separator>) {
  return <Menu.Separator className={cn(menuSeparatorClass, className)} {...props} />;
}

export const DropdownMenuRadioGroup = Menu.RadioGroup;

export function DropdownMenuRadioItem({
  className,
  children,
  ...props
}: ComponentProps<typeof Menu.RadioItem>) {
  return (
    <Menu.RadioItem className={cn(menuItemVariants({ inset: true }), className)} {...props}>
      <Menu.ItemIndicator className={menuIndicatorClass}>
        <CheckIcon />
      </Menu.ItemIndicator>
      {children}
    </Menu.RadioItem>
  );
}
