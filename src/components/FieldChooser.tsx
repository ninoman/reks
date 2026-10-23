/**
 * The chooser behind "+ Add filter", "+ Or…" and a row waiting for its
 * field: one search box over fields *and* values (`filterSearch.ts`). Blank,
 * it lists the fields by group, each My Tags category a field of its own; a
 * field that already shows on the level is greyed "shown" and picking it
 * jumps to its row (the store decides). Typed, it lists matching values
 * first ("Genre is Techno", "BPM around 128"), then fields. A combobox:
 * arrows move, Enter picks, Escape closes. Presentational.
 */
import {
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
  useId,
  useMemo,
  useState,
} from "react";

import { cn } from "../lib/cn";
import { fieldEntries, type SearchSource, searchFilters } from "../store/filterSearch";
import type { FieldKey, RowValue } from "../store/ruleTree";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Kbd } from "./ui/Kbd";
import { menuItemVariants, menuLabelClass } from "./ui/menu";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/Popover";
import type { OptionsStatus } from "./ValuePicker";

interface Option {
  id: string;
  /** For screen readers, when the parts of `content` would run together. */
  name?: string;
  content: ReactNode;
  muted: boolean;
  pick: () => void;
}

interface Section {
  label: string;
  /** A line instead of (or before) options: loading, error, nothing there. */
  note?: ReactNode;
  options: Option[];
}

export function FieldChooser({
  trigger,
  open,
  onOpenChange,
  shown,
  source,
  tagsStatus,
  onPick,
  onPickValue,
  onResetDefaults,
}: {
  trigger: ReactElement;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Fields that show on this level already. */
  shown: ReadonlySet<FieldKey>;
  /** Fields and values to search: the pickers' options. */
  source: SearchSource;
  tagsStatus: OptionsStatus;
  onPick: (key: FieldKey) => void;
  onPickValue: (value: RowValue) => void;
  /** Offers "Reset default fields" (the root level's Add filter). */
  onResetDefaults?: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const blank = query.trim() === "";

  const sections = useMemo((): Section[] => {
    const field = (key: FieldKey, label: string): Option => {
      const isShown = shown.has(key);
      return {
        id: `f-${key}`,
        muted: isShown,
        content: (
          <>
            {label}
            {isShown && <span className="ml-auto text-xs text-fg-subtle">shown</span>}
          </>
        ),
        pick: () => onPick(key),
      };
    };
    if (blank) {
      return fieldEntries(source.tags).map((g) => {
        let note: ReactNode;
        if (g.group === "tags") {
          if (tagsStatus.state === "loading") note = "Loading tags…";
          else if (tagsStatus.state === "error")
            note = (
              <>
                Couldn't load tags.{" "}
                <Button size="xs" variant="link" onClick={tagsStatus.retry}>
                  Retry
                </Button>
              </>
            );
          else if (g.entries.length === 0) note = "No tag categories";
        }
        return { label: g.label, note, options: g.entries.map((e) => field(e.key, e.label)) };
      });
    }
    const found = searchFilters(query, source);
    const values: Option[] = found.values.map((v) => ({
      id: `v-${v.id}`,
      name: `${v.field} ${v.verb} ${v.label}${v.detail === undefined ? "" : `, ${v.detail}`}`,
      muted: false,
      content: (
        <>
          <span className="text-fg-muted">
            {v.field} {v.verb}
          </span>
          <span className="truncate">{v.label}</span>
          {v.detail !== undefined && (
            <span className="ml-auto text-xs text-fg-subtle">{v.detail}</span>
          )}
        </>
      ),
      pick: () => onPickValue(v.value),
    }));
    const fields = found.fields.map((f) => field(f.key, f.label));
    return [
      ...(values.length > 0 ? [{ label: "Values", options: values }] : []),
      ...(fields.length > 0 ? [{ label: "Fields", options: fields }] : []),
    ];
  }, [blank, query, source, shown, tagsStatus, onPick, onPickValue]);

  const options = sections.flatMap((s) => s.options);
  const current = Math.min(active, options.length - 1);
  const optionId = (o: Option) => `${listId}-${o.id}`;

  function choose(o: Option) {
    o.pick();
    onOpenChange(false);
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (options.length === 0) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      setActive((current + step + options.length) % options.length);
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      setActive(e.key === "Home" ? 0 : options.length - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const o = options[current];
      if (o !== undefined) choose(o);
    }
  }

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        if (o) {
          setQuery("");
          setActive(0);
        }
        onOpenChange(o);
      }}
    >
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent
        align="start"
        className="flex max-h-(--radix-popover-content-available-height) w-64 flex-col gap-1"
        // The picked row takes focus (and opens its picker), not the trigger.
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        <Input
          variant="search"
          size="sm"
          type="search"
          role="combobox"
          aria-label="Find a field or value"
          aria-expanded
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={
            options[current] === undefined ? undefined : optionId(options[current])
          }
          placeholder="Field or value…"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        <div
          id={listId}
          role="listbox"
          aria-label="Filters to add"
          className="min-h-0 overflow-y-auto"
        >
          {sections.map((s) => (
            // biome-ignore lint/a11y/useSemanticElements: a listbox groups options with role="group"
            <div key={s.label} role="group" aria-label={s.label}>
              <div aria-hidden className={menuLabelClass}>
                {s.label}
              </div>
              {s.note !== undefined && (
                <p className="m-0 px-2 py-1 text-sm text-fg-muted">{s.note}</p>
              )}
              {s.options.map((o) => {
                const i = options.indexOf(o);
                return (
                  // Options stay unfocusable: focus is on the search box (aria-activedescendant).
                  // biome-ignore lint/a11y/useKeyWithClickEvents: the search box handles the keys
                  <div
                    key={o.id}
                    id={optionId(o)}
                    role="option"
                    aria-label={o.name}
                    tabIndex={-1}
                    aria-selected={i === current}
                    data-highlighted={i === current ? "" : undefined}
                    className={cn(menuItemVariants(), o.muted && "text-fg-muted")}
                    onMouseMove={() => setActive(i)}
                    // Keep focus in the search box.
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => choose(o)}
                  >
                    {o.content}
                  </div>
                );
              })}
            </div>
          ))}
          {!blank && options.length === 0 && (
            <p className="m-0 px-2 py-1 text-sm text-fg-muted">Nothing matches “{query.trim()}”.</p>
          )}
        </div>
        <div className="flex items-center gap-1.5 border-t border-border px-2 pt-1 text-xs text-fg-subtle">
          <Kbd aria-hidden>↵</Kbd> add
          <Kbd aria-hidden>esc</Kbd> close
          {blank && onResetDefaults !== undefined && (
            <Button
              size="xs"
              variant="link"
              className="ml-auto"
              onClick={() => {
                onResetDefaults();
                onOpenChange(false);
              }}
            >
              Reset default fields
            </Button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
