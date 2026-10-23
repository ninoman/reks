/**
 * A row's value picker in the Filters drawer: a button that sums up the
 * choice ("Any" while empty) and a popover with the operator on top (Any of
 * / None of, at least / at most…) and the choices below. `ListPicker` is the
 * usual body: a checklist with a search field, the count and Clear, and the
 * loading, error and empty states of its options.
 */
import { type ReactNode, useState } from "react";

import { cn } from "../lib/cn";
import { ReportErrorButton } from "./ErrorMessage";
import { ChevronDownIcon } from "./icons";
import { Button } from "./ui/Button";
import { Checkbox } from "./ui/Checkbox";
import { Input } from "./ui/Input";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/Popover";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";

/** Lists longer than this get a search field. */
const SEARCH_FROM = 8;

export interface PickerMode<M extends string> {
  value: M;
  label: string;
}

export interface PickerPopoverProps<M extends string> {
  /** The field, e.g. "Genre"; the button's name is "Genre: <summary>". */
  label: string;
  /** What is chosen; `null` shows "Any". */
  summary: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  modes?: readonly PickerMode<M>[];
  /** `null`: no mode chosen yet. */
  mode?: M | null;
  onModeChange?: (mode: M) => void;
  children: ReactNode;
}

export function PickerPopover<M extends string>({
  label,
  summary,
  open,
  onOpenChange,
  modes,
  mode = null,
  onModeChange,
  children,
}: PickerPopoverProps<M>) {
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <Button
          size="sm"
          className={cn(
            "min-w-0 max-w-36 justify-between font-normal",
            summary === null && "text-fg-muted",
          )}
          aria-label={`${label}: ${summary ?? "any"}`}
        >
          <span className="truncate">{summary ?? "Any"}</span>
          <ChevronDownIcon />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="flex w-60 flex-col gap-1.5">
        {modes !== undefined && (
          <SegmentedControl
            size="xs"
            aria-label={`${label}: match`}
            className="w-full"
            value={mode ?? ""}
            onValueChange={(v) => onModeChange?.(v as M)}
          >
            {modes.map((m) => (
              <Segment key={m.value} value={m.value} className="flex-1 justify-center">
                {m.label}
              </Segment>
            ))}
          </SegmentedControl>
        )}
        {children}
      </PopoverContent>
    </Popover>
  );
}

/** Where a picker's options come from: loading, failed (with Retry) or here. */
export type OptionsStatus =
  | { state: "loading" }
  | { state: "error"; error: string; retry: () => void }
  | { state: "ready" };

export interface ListOption<T> {
  value: T;
  label: string;
  /** Shown before the label (a colour swatch). */
  lead?: ReactNode;
}

export interface ListPickerProps<T, M extends string>
  extends Omit<PickerPopoverProps<M>, "children"> {
  options: readonly ListOption<T>[];
  selected: readonly T[];
  onChange: (selected: T[]) => void;
  /** What the options are, plural ("genres"): search field, states. */
  noun: string;
  status: OptionsStatus;
  /** Said when there are no options at all. */
  empty?: ReactNode;
}

export function ListPicker<T, M extends string>({
  options,
  selected,
  onChange,
  noun,
  status,
  empty,
  ...popover
}: ListPickerProps<T, M>) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const shown = q === "" ? options : options.filter((o) => o.label.toLowerCase().includes(q));
  const toggle = (v: T) =>
    onChange(selected.includes(v) ? selected.filter((s) => s !== v) : [...selected, v]);

  let body: ReactNode;
  if (status.state === "loading") {
    body = <p className="m-0 px-1.5 py-1 text-sm text-fg-muted">Loading…</p>;
  } else if (status.state === "error") {
    body = (
      <div className="flex flex-col gap-1.5 px-1.5 py-1 text-sm" role="alert">
        <span className="text-danger select-text">
          Couldn't load {noun}: {status.error}
        </span>
        <span className="flex gap-1.5">
          <Button size="sm" onClick={status.retry}>
            Retry
          </Button>
          <ReportErrorButton message={status.error} />
        </span>
      </div>
    );
  } else if (options.length === 0) {
    body = <p className="m-0 px-1.5 py-1 text-sm text-fg-muted">{empty ?? `No ${noun} yet.`}</p>;
  } else {
    body = (
      <>
        {options.length > SEARCH_FROM && (
          <Input
            variant="search"
            size="sm"
            type="search"
            aria-label={`Find ${noun}`}
            placeholder={`Search ${noun}`}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        )}
        <ul className="m-0 flex max-h-64 list-none flex-col overflow-y-auto p-0">
          {shown.map((o) => (
            <li key={String(o.value)}>
              <label className="flex h-(--menu-item-h) items-center gap-2 rounded-xs px-1.5 text-sm hover:bg-hover">
                <Checkbox checked={selected.includes(o.value)} onChange={() => toggle(o.value)} />
                {o.lead}
                <span className="truncate">{o.label}</span>
              </label>
            </li>
          ))}
        </ul>
        {shown.length === 0 && (
          <p className="m-0 px-1.5 py-1 text-sm text-fg-muted">
            No {noun} match “{query.trim()}”.
          </p>
        )}
        <div className="flex items-center justify-between border-t border-border px-1.5 pt-1 text-xs text-fg-muted">
          <span>{selected.length} selected</span>
          <Button
            size="xs"
            variant="link"
            disabled={selected.length === 0}
            onClick={() => onChange([])}
          >
            Clear
          </Button>
        </div>
      </>
    );
  }

  return (
    <PickerPopover
      {...popover}
      onOpenChange={(o) => {
        if (!o) setQuery("");
        popover.onOpenChange(o);
      }}
    >
      {body}
    </PickerPopover>
  );
}
