/**
 * Modal dialog (Radix): backdrop and focus trap; Escape goes to
 * `onOpenChange(false)`, and so does a click on the backdrop when
 * `closeOnOutsideClick` is set. Compose Header / Title / Body / Footer
 * inside `DialogContent`. A dialog is as tall as its content, up to its cap
 * (`height`), and animates when its size changes (`useAnimatedSize`); it fades in and
 * out (`useDialogMotion`).
 */
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, useCallback } from "react";

import { cn } from "../../lib/cn";
import { useAnimatedSize } from "./useAnimatedSize";
import { useDialogMotion } from "./useDialogMotion";

export const dialogContentVariants = cva(
  [
    "fixed top-1/2 left-1/2 z-(--z-dialog) flex max-h-[calc(100vh-64px)] w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2 flex-col",
    "rounded-lg border border-border bg-surface text-fg shadow-overlay outline-none",
  ],
  {
    variants: {
      size: { sm: "max-w-[360px]", md: "max-w-[560px]", lg: "max-w-[640px]", xl: "max-w-[960px]" },
      /** The tallest the dialog gets before its body scrolls (always within the window). */
      height: {
        auto: "",
        md: "max-h-[min(648px,calc(100vh-64px))]",
        lg: "max-h-[min(720px,calc(100vh-64px))]",
      },
    },
    defaultVariants: { size: "sm", height: "auto" },
  },
);

export const Dialog = DialogPrimitive.Root;

export function DialogContent({
  size,
  height,
  closeOnOutsideClick = false,
  className,
  children,
  onPointerDownOutside,
  ...props
}: ComponentProps<typeof DialogPrimitive.Content> &
  VariantProps<typeof dialogContentVariants> & { closeOnOutsideClick?: boolean }) {
  const sizeRef = useAnimatedSize();
  const motionRef = useDialogMotion();
  const ref = useCallback(
    (el: HTMLDivElement | null) => {
      sizeRef(el);
      motionRef(el);
    },
    [sizeRef, motionRef],
  );
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-(--z-dialog) bg-overlay" />
      <DialogPrimitive.Content
        ref={ref}
        aria-describedby={undefined}
        className={cn(dialogContentVariants({ size, height }), className)}
        onPointerDownOutside={(e) => {
          if (!closeOnOutsideClick) e.preventDefault();
          onPointerDownOutside?.(e);
        }}
        {...props}
      >
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

export function DialogHeader({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("flex flex-col gap-1 px-5 pt-4 pb-2", className)} {...props} />;
}

export function DialogTitle({ className, ...props }: ComponentProps<typeof DialogPrimitive.Title>) {
  return (
    <DialogPrimitive.Title
      className={cn("m-0 text-md font-semibold text-fg", className)}
      {...props}
    />
  );
}

export function DialogDescription({
  className,
  ...props
}: ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      className={cn("m-0 text-xs text-fg-muted", className)}
      {...props}
    />
  );
}

export function DialogBody({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      className={cn("flex min-h-0 flex-col gap-3 overflow-auto px-5 py-2", className)}
      {...props}
    />
  );
}

export function DialogFooter({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("flex justify-end gap-2 px-5 pt-3 pb-4", className)} {...props} />;
}
