/**
 * A small in-app modal (the WebView's `window.confirm` differs per OS).
 * Escape picks the last button, which is always the cancel choice.
 */
import { type ReactNode, useEffect, useRef } from "react";

export interface DialogButton {
  label: string;
  onClick: () => void;
  primary?: boolean;
}

export function Dialog({
  title,
  children,
  buttons,
}: {
  title: string;
  children: ReactNode;
  buttons: DialogButton[];
}) {
  const ref = useRef<HTMLDivElement>(null);
  const cancel = buttons[buttons.length - 1];

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button.primary")?.focus();
  }, []);

  return (
    <div className="dialog-backdrop">
      <div
        ref={ref}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            cancel?.onClick();
          }
        }}
      >
        <h3>{title}</h3>
        <div className="dialog-body">{children}</div>
        <div className="dialog-buttons">
          {buttons.map((b) => (
            <button
              key={b.label}
              type="button"
              className={b.primary ? "primary" : undefined}
              onClick={b.onClick}
            >
              {b.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
