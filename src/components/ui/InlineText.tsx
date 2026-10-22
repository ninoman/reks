/**
 * A short text (a name) edited in place, like `InlineNumber`: double-click
 * the text (or click, with `editOn="click"`), or Enter while it has focus, to type. Enter or leaving the
 * field commits, Esc cancels. The committed text is trimmed; blank commits
 * `null` (no name). `InlineTextField` is the field alone, for a host that
 * opens it itself (a hot cue pad on double-click).
 */
import { type KeyboardEvent, type ReactNode, useState } from "react";

import { cn } from "../../lib/cn";
import { disabledState, fieldSurface, focusRing } from "./control";

/** Typed text as stored: trimmed, null when blank. */
export function cleanText(text: string): string | null {
  const t = text.trim();
  return t === "" ? null : t;
}

export interface InlineTextFieldProps {
  /** The text the field starts with. */
  initial: string;
  /** `commit` false: Esc, nothing to save. */
  onDone: (commit: boolean, value: string | null) => void;
  maxLength?: number | undefined;
  "aria-label": string;
  placeholder?: string | undefined;
  className?: string | undefined;
}

export function InlineTextField({
  initial,
  onDone,
  maxLength,
  "aria-label": label,
  placeholder,
  className,
}: InlineTextFieldProps) {
  const [draft, setDraft] = useState(initial);
  // Enter or Esc closes the field; the blur that follows must not commit again.
  const [closed, setClosed] = useState(false);

  function finish(commit: boolean) {
    if (closed) return;
    setClosed(true);
    onDone(commit, cleanText(draft));
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

  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: the field opens because the user asked to type
      autoFocus
      aria-label={label}
      value={draft}
      maxLength={maxLength}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onFocus={(e) => e.target.select()}
      onBlur={() => finish(true)}
      onKeyDown={onKeyDown}
      className={cn(
        "w-full min-w-0 rounded-xs px-0.5 select-text",
        fieldSurface,
        focusRing,
        className,
      )}
    />
  );
}

export interface InlineTextProps {
  value: string | null;
  /** Shown when `value` is null. */
  placeholder?: ReactNode;
  onCommit: (value: string | null) => void;
  maxLength?: number | undefined;
  disabled?: boolean;
  "aria-label": string;
  /** Native `title` hint for the shown text. */
  title?: string;
  /** What opens the field; a plain click suits rows where a click does nothing else. */
  editOn?: "click" | "doubleClick";
  className?: string | undefined;
}

export function InlineText({
  value,
  placeholder = "—",
  onCommit,
  maxLength,
  disabled = false,
  "aria-label": label,
  title,
  editOn = "doubleClick",
  className,
}: InlineTextProps) {
  const [editing, setEditing] = useState(false);

  if (editing) {
    return (
      <InlineTextField
        initial={value ?? ""}
        maxLength={maxLength}
        aria-label={label}
        className={className}
        onDone={(commit, v) => {
          setEditing(false);
          if (commit && v !== value) onCommit(v);
        }}
      />
    );
  }
  return (
    <button
      type="button"
      aria-label={label}
      title={title}
      disabled={disabled}
      onClick={editOn === "click" ? () => setEditing(true) : undefined}
      onDoubleClick={editOn === "doubleClick" ? () => setEditing(true) : undefined}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          setEditing(true);
        }
      }}
      className={cn(
        "w-full min-w-0 cursor-default truncate rounded-xs border border-transparent px-0.5 text-left",
        editOn === "click" && !disabled && "cursor-text hover:bg-hover",
        focusRing,
        disabledState,
        value === null && "text-fg-subtle",
        className,
      )}
    >
      {value ?? placeholder}
    </button>
  );
}
