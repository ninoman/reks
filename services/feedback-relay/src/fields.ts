/**
 * Which board fields to set on a new item. Pure: the board's field list
 * comes in, field and option ids go out. Matching is by name, ignoring case,
 * so renaming an option on the board only needs the same name here.
 */
import type { ReportKind } from "./payload";

export interface SelectField {
  id: string;
  name: string;
  options: { id: string; name: string }[];
}

export interface FieldValue {
  fieldId: string;
  optionId: string;
}

export interface FieldConfig {
  typeField: string;
  statusField: string;
  statusValue: string;
}

const KIND_OPTION: Record<ReportKind, string> = {
  bug: "Bug",
  feature: "Feature",
  feedback: "Feedback",
};

function same(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

function pick(fields: SelectField[], field: string, option: string): FieldValue | null {
  const f = fields.find((x) => same(x.name, field));
  const o = f?.options.find((x) => same(x.name, option));
  return f && o ? { fieldId: f.id, optionId: o.id } : null;
}

/** Kind and status values for a report; a missing field or option is skipped. */
export function fieldValues(
  fields: SelectField[],
  kind: ReportKind,
  cfg: FieldConfig,
): FieldValue[] {
  return [
    pick(fields, cfg.typeField, KIND_OPTION[kind]),
    pick(fields, cfg.statusField, cfg.statusValue),
  ].filter((v): v is FieldValue => v !== null);
}
