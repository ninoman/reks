/**
 * A small in-app question (the WebView's `window.confirm` differs per OS).
 * Escape picks the last button, which is always the cancel choice; the
 * primary button has focus when it opens.
 */
import type { ReactNode } from "react";

import { Button } from "./ui/Button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";

export interface DialogButton {
  label: string;
  onClick: () => void;
  primary?: boolean;
}

export function ConfirmDialog({
  title,
  children,
  buttons,
}: {
  title: string;
  children: ReactNode;
  buttons: DialogButton[];
}) {
  const cancel = buttons[buttons.length - 1];
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) cancel?.onClick();
      }}
    >
      <DialogContent
        size="sm"
        onOpenAutoFocus={(e) => {
          const primary = (e.currentTarget as HTMLElement).querySelector<HTMLElement>(
            "[data-primary]",
          );
          if (primary !== null) {
            e.preventDefault();
            primary.focus();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <DialogBody className="text-sm">{children}</DialogBody>
        <DialogFooter>
          {buttons.map((b) => (
            <Button
              key={b.label}
              variant={b.primary ? "primary" : "secondary"}
              data-primary={b.primary || undefined}
              onClick={b.onClick}
            >
              {b.label}
            </Button>
          ))}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
