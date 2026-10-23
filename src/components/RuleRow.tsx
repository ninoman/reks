/**
 * One row of the Filters drawer: its dot on the rail, the field, the value
 * picker, × and the ··· menu (Or…, Exclude, Pause, Keep in default fields,
 * Remove). A row that filters is outlined; a paused one is dashed and struck
 * through. Presentational: the builder passes the row and the actions.
 *
 * The dot is the row's keyboard handle: ↑ / ↓ move between rows, Enter
 * opens the value, Space pauses, Backspace / Delete empties a set row or
 * removes an empty one, Shift+F10 opens the menu.
 */
import { type KeyboardEvent, useEffect, useRef, useState } from "react";

import { cn } from "../lib/cn";
import type { RowFocus } from "../store/rules";
import {
  type FieldKey,
  fieldKey,
  isRowActive,
  isRowSet,
  type RowDraft,
  type RowValue,
} from "../store/ruleTree";
import { FieldChooser } from "./FieldChooser";
import { CloseIcon, MoreIcon } from "./icons";
import { type RowPatch, type RuleChoices, RuleValueEditor } from "./RuleValueEditor";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";
import { IconButton } from "./ui/IconButton";

export interface RuleRowActions {
  updateRow(rowId: string, patch: RowPatch & { paused?: boolean }): void;
  clearRow(rowId: string): void;
  remove(rowId: string): void;
  or(rowId: string): void;
  chooseField(rowId: string, key: FieldKey): void;
  chooseValue(rowId: string, value: RowValue): void;
  keepDefault(key: FieldKey, keep: boolean): void;
  clearFocus(): void;
}

/** A hint at the right of a menu item. */
function Hint({ children }: { children: string }) {
  return <span className="ml-auto pl-3 text-xs text-fg-subtle">{children}</span>;
}

export function RuleRow({
  row,
  label,
  isTag,
  choices,
  actions,
  focus,
  isDefault,
  keepsPlace,
  canOr,
}: {
  row: RowDraft;
  /** The field's name; `null` while the row waits for its field. */
  label: string | null;
  /** A My Tags category (marked #, apart from the Genre field). */
  isTag: boolean;
  choices: RuleChoices;
  actions: RuleRowActions;
  /** Set when this row should take focus. */
  focus: RowFocus | null;
  /** Its field is one of the default fields. */
  isDefault: boolean;
  /** A default row on the root level: × empties it instead of removing it. */
  keepsPlace: boolean;
  canOr: boolean;
}) {
  const handleRef = useRef<HTMLButtonElement>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [chooserOpen, setChooserOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const waiting = row.value.field === null;

  useEffect(() => {
    if (focus === null) return;
    handleRef.current?.focus();
    if (focus.open) (waiting ? setChooserOpen : setPickerOpen)(true);
    actions.clearFocus();
  }, [focus, actions, waiting]);

  const key = fieldKey(row.value);
  const set = isRowSet(row);
  const active = isRowActive(row);
  const name = label === null ? "New filter" : isTag ? `${label} (My Tags)` : label;
  const v = row.value;

  function onKeyDown(e: KeyboardEvent<HTMLButtonElement>) {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const handles = [
        ...(e.currentTarget
          .closest("[data-query]")
          ?.querySelectorAll<HTMLElement>("[data-row-handle]") ?? []),
      ];
      const at = handles.indexOf(e.currentTarget);
      handles[at + (e.key === "ArrowUp" ? -1 : 1)]?.focus();
    } else if (e.key === "Enter") {
      e.preventDefault();
      (waiting ? setChooserOpen : setPickerOpen)(true);
    } else if (e.key === "Backspace" || e.key === "Delete") {
      e.preventDefault();
      if (set) actions.clearRow(row.id);
      else actions.remove(row.id);
    } else if ((e.key === "F10" && e.shiftKey) || e.key === "ContextMenu") {
      e.preventDefault();
      setMenuOpen(true);
    }
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a div group keeps the row's controls in one flex line
    <div
      role="group"
      aria-label={`${name} filter`}
      data-active={active || undefined}
      data-paused={row.paused || undefined}
      className={cn(
        "relative flex min-h-(--query-row-h) items-center gap-1 rounded-sm border border-transparent py-0.5 pr-0.5 pl-1.5",
        "has-[[data-row-handle]:focus-visible]:outline-solid has-[[data-row-handle]:focus-visible]:outline-(length:--focus-ring-width) has-[[data-row-handle]:focus-visible]:outline-focus-ring",
        active && "border-accent bg-surface",
        row.paused && "border-dashed border-fg-subtle",
      )}
    >
      <button
        ref={handleRef}
        type="button"
        data-row-handle
        aria-label={`${name}${row.paused ? ", paused" : ""}`}
        aria-keyshortcuts="Enter Space Backspace Shift+F10"
        title={set ? (row.paused ? "Resume (Space)" : "Pause (Space)") : undefined}
        className="absolute inset-y-0 left-[calc(var(--query-rail-w)*-1)] flex w-(--query-rail-w) cursor-default items-center justify-center outline-hidden"
        onClick={() => set && actions.updateRow(row.id, { paused: !row.paused })}
        onKeyDown={onKeyDown}
      >
        <span
          className={cn(
            "size-(--query-dot) rounded-full border border-fg-subtle bg-bg-subtle",
            active && "border-accent bg-accent",
            row.paused && "border-dashed",
          )}
        />
      </button>

      {v.field === null ? (
        <FieldChooser
          trigger={
            <Button size="sm" variant="ghost" className="flex-1 justify-start text-fg-muted">
              Choose a field…
            </Button>
          }
          open={chooserOpen}
          onOpenChange={setChooserOpen}
          shown={new Set()}
          source={choices}
          tagsStatus={choices.tagsStatus}
          onPick={(k) => actions.chooseField(row.id, k)}
          onPickValue={(value) => actions.chooseValue(row.id, value)}
        />
      ) : (
        <>
          <span
            className={cn(
              "w-(--query-label-w) shrink-0 truncate text-sm text-fg-muted",
              active && "font-semibold text-fg",
              row.paused && "line-through opacity-55",
            )}
          >
            {isTag && (
              <span
                aria-hidden
                className={cn("mr-0.5 text-2xs font-bold text-fg-subtle", active && "text-accent")}
              >
                #
              </span>
            )}
            {label}
          </span>
          <span
            className={cn(
              "flex min-w-0 flex-1 items-center justify-end gap-1",
              row.paused && "opacity-55",
            )}
          >
            {row.not && (
              <Badge variant="danger" className="uppercase">
                not
              </Badge>
            )}
            {v.field === "tag" && v.all && !row.not && (
              <Badge variant="outline" className="uppercase">
                all
              </Badge>
            )}
            <RuleValueEditor
              label={name}
              row={row}
              choices={choices}
              open={pickerOpen}
              onOpenChange={setPickerOpen}
              onChange={(patch) => actions.updateRow(row.id, patch)}
            />
          </span>
        </>
      )}

      {set && (
        <IconButton
          size="xs"
          aria-label={keepsPlace ? `Clear ${name}` : `Remove ${name}`}
          title={keepsPlace ? "Clear" : "Remove"}
          onClick={() => (keepsPlace ? actions.clearRow(row.id) : actions.remove(row.id))}
        >
          <CloseIcon />
        </IconButton>
      )}

      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <IconButton size="xs" aria-label={`${name} options`} title="Options (Shift+F10)">
            <MoreIcon />
          </IconButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-52">
          <DropdownMenuItem inset disabled={!canOr} onSelect={() => actions.or(row.id)}>
            Or…
            <Hint>this or another filter</Hint>
          </DropdownMenuItem>
          <DropdownMenuCheckboxItem
            checked={row.not}
            disabled={v.field === null}
            onCheckedChange={(not) =>
              actions.updateRow(row.id, {
                not,
                // "Not all of" can't be said; Exclude means none of them.
                ...(v.field === "tag" && v.all ? { value: { ...v, all: false } } : {}),
              })
            }
          >
            Exclude
            <Hint>none of these</Hint>
          </DropdownMenuCheckboxItem>
          <DropdownMenuCheckboxItem
            checked={row.paused}
            disabled={!set}
            onCheckedChange={(paused) => actions.updateRow(row.id, { paused })}
          >
            Pause
            <Hint>keep, don't apply</Hint>
          </DropdownMenuCheckboxItem>
          <DropdownMenuSeparator />
          <DropdownMenuCheckboxItem
            checked={isDefault}
            disabled={key === null}
            onCheckedChange={(keep) => key !== null && actions.keepDefault(key, keep)}
          >
            Keep in default fields
          </DropdownMenuCheckboxItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem inset variant="danger" onSelect={() => actions.remove(row.id)}>
            Remove
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
