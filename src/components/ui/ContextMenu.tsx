/**
 * Right-click menu (Radix). Wrap the whole area (table, tree), not each row.
 * Rows may note what was hit in their own `onContextMenu`, but must not call
 * `preventDefault`: Radix then skips opening.
 */
import * as Menu from "@radix-ui/react-context-menu";
import type { VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { CheckIcon, ChevronRightIcon, MinusIcon } from "../icons";
import {
  menuContentVariants,
  menuIndicatorClass,
  menuItemVariants,
  menuLabelClass,
  menuSeparatorClass,
} from "./menu";

export { menuContentVariants as contextMenuContentVariants };
export const ContextMenu = Menu.Root;
export const ContextMenuTrigger = Menu.Trigger;
export const ContextMenuSub = Menu.Sub;
export const ContextMenuGroup = Menu.Group;

/** Heading of a `ContextMenuGroup`. */
export function ContextMenuLabel({ className, ...props }: ComponentProps<typeof Menu.Label>) {
  return <Menu.Label className={cn(menuLabelClass, className)} {...props} />;
}

export function ContextMenuContent({ className, ...props }: ComponentProps<typeof Menu.Content>) {
  return (
    <Menu.Portal>
      <Menu.Content className={cn(menuContentVariants(), className)} {...props} />
    </Menu.Portal>
  );
}

export function ContextMenuItem({
  className,
  variant,
  inset,
  ...props
}: ComponentProps<typeof Menu.Item> & VariantProps<typeof menuItemVariants>) {
  return <Menu.Item className={cn(menuItemVariants({ variant, inset }), className)} {...props} />;
}

export function ContextMenuSeparator({
  className,
  ...props
}: ComponentProps<typeof Menu.Separator>) {
  return <Menu.Separator className={cn(menuSeparatorClass, className)} {...props} />;
}

export function ContextMenuSubTrigger({
  className,
  children,
  ...props
}: ComponentProps<typeof Menu.SubTrigger>) {
  return (
    <Menu.SubTrigger
      className={cn(menuItemVariants(), "data-[state=open]:bg-hover", className)}
      {...props}
    >
      {children}
      <ChevronRightIcon />
    </Menu.SubTrigger>
  );
}

export function ContextMenuSubContent({
  className,
  ...props
}: ComponentProps<typeof Menu.SubContent>) {
  return (
    <Menu.Portal>
      <Menu.SubContent className={cn(menuContentVariants(), className)} {...props} />
    </Menu.Portal>
  );
}

/** `checked="indeterminate"` shows a dash (some of the selection has it). */
export function ContextMenuCheckboxItem({
  className,
  children,
  checked,
  ...props
}: ComponentProps<typeof Menu.CheckboxItem>) {
  return (
    <Menu.CheckboxItem
      className={cn(menuItemVariants({ inset: true }), className)}
      checked={checked}
      {...props}
    >
      <Menu.ItemIndicator className={menuIndicatorClass}>
        {checked === "indeterminate" ? <MinusIcon /> : <CheckIcon />}
      </Menu.ItemIndicator>
      {children}
    </Menu.CheckboxItem>
  );
}
