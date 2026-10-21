/** A small message pinned bottom-centre, with an optional action button (Undo). */
import type { ComponentProps, ReactNode } from "react";

import { cn } from "../../lib/cn";
import { Button } from "./Button";

export type ToastProps = Omit<ComponentProps<"div">, "children"> & {
  children: ReactNode;
  action?: { label: string; onClick: () => void } | undefined;
  onDismiss: () => void;
};

export function Toast({ children, action, onDismiss, className, ...props }: ToastProps) {
  return (
    <div
      role="status"
      className={cn(
        "fixed bottom-14 left-1/2 z-(--z-toast) flex -translate-x-1/2 items-center gap-2 rounded-md bg-fg py-1 pr-1 pl-3 text-sm text-bg shadow-overlay",
        className,
      )}
      {...props}
    >
      <span>{children}</span>
      {action !== undefined && (
        <Button size="sm" variant="secondary" onClick={action.onClick}>
          {action.label}
        </Button>
      )}
      <Button size="sm" variant="quiet" className="text-bg/70 hover:text-bg" onClick={onDismiss}>
        Dismiss
      </Button>
    </div>
  );
}
