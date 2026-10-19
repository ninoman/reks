/**
 * Right-click menu (Radix). Wrap the whole area (table, tree), not each row.
 * Rows may note what was hit in their own `onContextMenu`, but must not call
 * `preventDefault`: Radix then skips opening.
 */
import * as Menu from "@radix-ui/react-context-menu";
import type { VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";
import { menuContentVariants, menuItemVariants, menuSeparatorClass } from "./menu";

export { menuContentVariants as contextMenuContentVariants };
export const ContextMenu = Menu.Root;
export const ContextMenuTrigger = Menu.Trigger;

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
