/**
 * A number shown as text that turns into a field to type a new one:
 * double-click it, or Enter while it has focus. Enter or leaving the field
 * commits, Esc cancels; text that is not a number is dropped. The shown
 * text is a button, so it is reachable with Tab.
 */
import { type KeyboardEvent, type ReactNode, useState } from "react";

import { cn } from "../../lib/cn";
import { disabledState, fieldSurface, focusRing } from "./control";

export interface InlineNumberProps {
  value: number | null;
  /** What shows when not editing (and in the field when editing starts). */
  format: (value: number) => string;
  /** Shown when `value` is null. */
  placeholder?: ReactNode;
  onCommit: (value: number) => void;
  disabled?: boolean;
  "aria-label": string;
  /** Tooltip-style hint for the shown text (native `title`). */
  title?: string;
  className?: string;
}

/** The typed text as a number, or null ("131,5" reads as 131.5). */
export function parseNumber(text: string): number | null {
  const t = text.trim().replace(",", ".");
  if (t === "" || !/^[-+]?\d*\.?\d*$/.test(t)) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

export function InlineNumber({
  value,
  format,
  placeholder = "—",
  onCommit,
  disabled = false,
  "aria-label": label,
  title,
  className,
}: InlineNumberProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const editable = !disabled && value !== null;

  function start() {
    if (editable) setDraft(format(value));
  }

  function finish(commit: boolean) {
    if (draft === null) return;
    const n = commit ? parseNumber(draft) : null;
    setDraft(null);
    if (n !== null) onCommit(n);
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      finish(true);
    } else if (e.key === "Escape") {
      // Esc closes the field, not the dialog or panel around it.
      e.preventDefault();
      e.stopPropagation();
      finish(false);
    }
  }

  if (draft !== null) {
    return (
      <input
        // biome-ignore lint/a11y/noAutofocus: the field opens because the user asked to type
        autoFocus
        inputMode="decimal"
        aria-label={label}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onFocus={(e) => e.target.select()}
        onBlur={() => finish(true)}
        onKeyDown={onKeyDown}
        className={cn(
          "w-full rounded-xs px-0.5 text-right select-text",
          fieldSurface,
          focusRing,
          className,
        )}
      />
    );
  }
  return (
    <button
      type="button"
      aria-label={label}
      title={title}
      disabled={!editable}
      onDoubleClick={start}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          start();
        }
      }}
      className={cn(
        "w-full cursor-default rounded-xs border border-transparent px-0.5 text-right",
        focusRing,
        disabledState,
        "disabled:opacity-100",
        className,
      )}
    >
      {value === null ? placeholder : format(value)}
    </button>
  );
}
