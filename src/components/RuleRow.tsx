/**
 * One condition: field, is / is not, value, remove. Presentational: the
 * builder passes the row and the actions. An unfinished row (a field, no
 * value yet) is muted and says it is not applied.
 *
 * Keys: Enter in the row's fields adds a row below; Delete or Backspace on
 * the field Select of a row with no field removes the row.
 */
import { type KeyboardEvent, useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import {
  isRowComplete,
  negationLabels,
  type RowDraft,
  type RowValue,
  RULE_FIELDS,
  type RuleField,
} from "../store/ruleTree";
import { CloseIcon } from "./icons";
import { type RuleChoices, RuleValueEditor } from "./RuleValueEditor";
import { IconButton } from "./ui/IconButton";
import { Select } from "./ui/Select";

export interface RuleRowActions {
  setField(rowId: string, field: RuleField | null): void;
  setNot(rowId: string, not: boolean): void;
  setValue(rowId: string, value: RowValue): void;
  remove(rowId: string): void;
  addRowAfter(rowId: string): void;
}

export function RuleRow({
  row,
  choices,
  actions,
  autoFocus,
  onFocused,
  removable,
}: {
  row: RowDraft;
  choices: RuleChoices;
  actions: RuleRowActions;
  /** Take focus on the field Select (a row that was just added). */
  autoFocus: boolean;
  onFocused: () => void;
  /** False for the only row of the builder. */
  removable: boolean;
}) {
  const fieldRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (!autoFocus) return;
    fieldRef.current?.focus();
    onFocused();
  }, [autoFocus, onFocused]);

  const field = row.value.field;
  const [yes, no] = negationLabels(field);
  const unfinished = field !== null && !isRowComplete(row);

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const t = e.target;
    if (e.key === "Enter" && (t instanceof HTMLInputElement || t instanceof HTMLSelectElement)) {
      e.preventDefault();
      actions.addRowAfter(row.id);
    } else if (
      (e.key === "Delete" || e.key === "Backspace") &&
      t === fieldRef.current &&
      field === null &&
      removable
    ) {
      e.preventDefault();
      actions.remove(row.id);
    }
  }

  return (
    // The keys are shortcuts on the row's own controls; each control keeps its normal keyboard use.
    // biome-ignore lint/a11y/useSemanticElements: a div group keeps the row's controls in one flex line
    <div
      role="group"
      aria-label={field === null ? "Condition" : `Condition: ${field}`}
      className={cn("flex flex-wrap items-center gap-1.5", unfinished && "opacity-60")}
      onKeyDown={onKeyDown}
    >
      <Select
        ref={fieldRef}
        size="sm"
        aria-label="Field"
        className="w-28"
        value={field ?? ""}
        onChange={(e) =>
          actions.setField(row.id, e.target.value === "" ? null : (e.target.value as RuleField))
        }
      >
        <option value="">Choose…</option>
        {RULE_FIELDS.map((f) => (
          <option key={f.field} value={f.field}>
            {f.label}
          </option>
        ))}
      </Select>
      <Select
        size="sm"
        aria-label="Is or is not"
        className="w-32"
        value={row.not ? "not" : "is"}
        onChange={(e) => actions.setNot(row.id, e.target.value === "not")}
      >
        <option value="is">{yes}</option>
        <option value="not">{no}</option>
      </Select>
      <RuleValueEditor
        value={row.value}
        choices={choices}
        onChange={(v) => actions.setValue(row.id, v)}
      />
      {unfinished && <span className="text-xs text-fg-muted">Not applied yet</span>}
      <IconButton
        size="xs"
        aria-label="Remove condition"
        title="Remove condition"
        className="ml-auto"
        disabled={!removable}
        onClick={() => actions.remove(row.id)}
      >
        <CloseIcon />
      </IconButton>
    </div>
  );
}
