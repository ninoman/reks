/**
 * Form model for the bulk edit of the selected tracks: what is shared or
 * "Mixed", which fields the user has touched, the `BulkPatch` they make and
 * how many tracks it changes. Only touched fields are sent. Values are passed
 * through as typed; validation lives in `reks-core` (`bulk_edit.rs`).
 */
import type { BulkPatch, TagId, TrackRow } from "../lib/ipc";

export type BulkField =
  | "artist"
  | "album"
  | "genre"
  | "label"
  | "year"
  | "rating"
  | "color"
  | "key";

export const BULK_TEXT_FIELDS: { id: BulkField; label: string }[] = [
  { id: "artist", label: "Artist" },
  { id: "album", label: "Album" },
  { id: "genre", label: "Genre" },
  { id: "label", label: "Label" },
];

/** At or above this many tracks, Apply asks first. */
export const BULK_CONFIRM_FROM = 20;

export type CommentMode = "replace" | "append";

export interface BulkForm {
  /** Touched fields only, as the input's string; `""` clears. */
  fields: Partial<Record<BulkField, string>>;
  /** `null` until the user types; the mode is kept meanwhile in `commentMode`. */
  comment: { mode: CommentMode; text: string } | null;
  commentMode: CommentMode;
  /** Tags to add to or remove from every track. */
  tags: ReadonlyMap<TagId, "add" | "remove">;
}

export const emptyBulkForm: BulkForm = {
  fields: {},
  comment: null,
  commentMode: "replace",
  tags: new Map(),
};

export type Shared = { kind: "same"; value: string } | { kind: "mixed" };

function cellText(row: TrackRow, field: BulkField | "comment"): string {
  const v = row[field];
  return v === null ? "" : String(v);
}

/** The value every row has for `field`, or "mixed". */
export function sharedValue(rows: readonly TrackRow[], field: BulkField | "comment"): Shared {
  const [first, ...rest] = rows;
  if (first === undefined) return { kind: "same", value: "" };
  const value = cellText(first, field);
  return rest.every((r) => cellText(r, field) === value)
    ? { kind: "same", value }
    : { kind: "mixed" };
}

/** Sets a field; typing back the shared value counts as untouched. */
export function setField(
  rows: readonly TrackRow[],
  form: BulkForm,
  field: BulkField,
  value: string,
): BulkForm {
  const shared = sharedValue(rows, field);
  const fields = { ...form.fields };
  if (shared.kind === "same" && shared.value === value) delete fields[field];
  else fields[field] = value;
  return { ...form, fields };
}

export function setCommentText(form: BulkForm, text: string): BulkForm {
  return { ...form, comment: { mode: form.commentMode, text } };
}

export function setCommentMode(form: BulkForm, mode: CommentMode): BulkForm {
  return {
    ...form,
    commentMode: mode,
    comment: form.comment === null ? null : { ...form.comment, mode },
  };
}

/**
 * A click on a tag: untouched → add it (or remove it when every track has
 * it), touched → back to untouched. `have` is how many tracks carry it.
 */
export function toggleTag(form: BulkForm, tag: TagId, tracks: number, have: number): BulkForm {
  const tags = new Map(form.tags);
  if (tags.has(tag)) tags.delete(tag);
  else tags.set(tag, have === tracks ? "remove" : "add");
  return { ...form, tags };
}

export function buildBulkPatch(form: BulkForm): BulkPatch {
  const patch: BulkPatch = {};
  const p = patch as Record<string, string | number | null>;
  for (const [field, v] of Object.entries(form.fields) as [BulkField, string][]) {
    if (field === "rating") p[field] = Number(v);
    else if (field === "year") p[field] = v.trim() === "" ? null : Number(v);
    else p[field] = v.trim() === "" ? null : v;
  }
  const c = form.comment;
  if (c?.mode === "replace")
    patch.comment = { mode: "replace", text: c.text === "" ? null : c.text };
  else if (c?.mode === "append" && c.text.trim() !== "") {
    patch.comment = { mode: "append", text: c.text };
  }
  const add = [...form.tags].filter(([, a]) => a === "add").map(([t]) => t);
  const remove = [...form.tags].filter(([, a]) => a === "remove").map(([t]) => t);
  if (add.length > 0) patch.addTags = add;
  if (remove.length > 0) patch.removeTags = remove;
  return patch;
}

/** Whether `row` would differ after `patch`. `keyLabel`: the key as the user types it. */
function changes(row: TrackRow, patch: BulkPatch, keyLabel: (key: string) => string): boolean {
  const text = (v: string | null | undefined) => (v ?? "").trim();
  for (const f of ["artist", "album", "genre", "label", "color"] as const) {
    const v = patch[f];
    if (v !== undefined && text(v) !== text(row[f])) return true;
  }
  if (patch.year !== undefined && patch.year !== row.year) return true;
  if (patch.rating !== undefined && patch.rating !== row.rating) return true;
  if (patch.key !== undefined && text(patch.key) !== (row.key === null ? "" : keyLabel(row.key))) {
    return true;
  }
  const c = patch.comment;
  if (c?.mode === "append") return true;
  if (c?.mode === "replace" && text(c.text) !== text(row.comment)) return true;
  if (patch.addTags?.some((t) => !row.tags.includes(t))) return true;
  return patch.removeTags?.some((t) => row.tags.includes(t)) ?? false;
}

/** How many of `rows` the form would change. */
export function countChanged(
  rows: readonly TrackRow[],
  form: BulkForm,
  keyLabel: (key: string) => string = (k) => k,
): number {
  const patch = buildBulkPatch(form);
  return rows.filter((r) => changes(r, patch, keyLabel)).length;
}

const FIELD_LABELS: Record<BulkField, string> = {
  artist: "Artist",
  album: "Album",
  genre: "Genre",
  label: "Label",
  year: "Year",
  rating: "Rating",
  color: "Colour",
  key: "Key",
};

/** What the form changes, for the confirmation: field names, comment mode, tag counts. */
export function changeSummary(form: BulkForm): string[] {
  const patch = buildBulkPatch(form);
  const out = (Object.keys(FIELD_LABELS) as BulkField[])
    .filter((f) => patch[f] !== undefined)
    .map((f) => FIELD_LABELS[f]);
  if (patch.comment !== undefined) {
    out.push(patch.comment.mode === "append" ? "Comment (append)" : "Comment (replace)");
  }
  if (patch.addTags !== undefined) out.push(`Add ${patch.addTags.length} tag(s)`);
  if (patch.removeTags !== undefined) out.push(`Remove ${patch.removeTags.length} tag(s)`);
  return out;
}
