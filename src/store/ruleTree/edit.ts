/** Pure edits of a draft; each returns a new tree. */
import type { TagId } from "../../lib/ipc";
import {
  emptyValue,
  type FieldKey,
  fieldKey,
  type GroupDraft,
  type GroupMode,
  type ItemDraft,
  newGroup,
  newRow,
  type RowDraft,
  type RowValue,
} from "./model";
import { canOr, findRow, hiddenRows, isUnusedDefault, parentOf } from "./query";
import { isRowSet } from "./toRule";

// ---- edits -----------------------------------------------------------------

function mapItems(
  group: GroupDraft,
  f: (i: ItemDraft) => ItemDraft | ItemDraft[] | null,
): GroupDraft {
  return {
    ...group,
    rules: group.rules.flatMap((i) => {
      const next = f(i);
      const list = next === null ? [] : Array.isArray(next) ? next : [next];
      return list.map((n) => (n.kind === "group" && n === i ? mapItems(n, f) : n));
    }),
  };
}

function inGroup(root: GroupDraft, groupId: string, edit: (g: GroupDraft) => GroupDraft) {
  if (root.id === groupId) return edit(root);
  return mapItems(root, (i) => (i.kind === "group" && i.id === groupId ? edit(i) : i));
}

/** Where a row added to `group` goes: after the last row that isn't an unused default. */
export function insertIndex(root: GroupDraft, group: GroupDraft): number {
  if (group !== root) return group.rules.length;
  let at = 0;
  group.rules.forEach((item, i) => {
    if (!isUnusedDefault(item)) at = i + 1;
  });
  return at;
}

function insertInto(root: GroupDraft, groupId: string, item: ItemDraft): GroupDraft {
  return inGroup(root, groupId, (g) => {
    const at = insertIndex(root, g);
    return { ...g, rules: [...g.rules.slice(0, at), item, ...g.rules.slice(at)] };
  });
}

/** Removes a row or group (not the root). */
export function removeItem(root: GroupDraft, id: string): GroupDraft {
  return mapItems(root, (i) => (i.id === id ? null : i));
}

export function setMode(root: GroupDraft, groupId: string, mode: GroupMode): GroupDraft {
  return inGroup(root, groupId, (g) => ({ ...g, mode }));
}

/** A connector click: "and" becomes "or" for the whole level, and back. */
export function toggleMode(root: GroupDraft, groupId: string): GroupDraft {
  return inGroup(root, groupId, (g) => ({ ...g, mode: g.mode === "all" ? "any" : "all" }));
}

export function updateRow(
  root: GroupDraft,
  rowId: string,
  patch: Partial<Pick<RowDraft, "not" | "paused" | "value">>,
): GroupDraft {
  return mapItems(root, (i) => (i.kind === "row" && i.id === rowId ? { ...i, ...patch } : i));
}

/** × on a row: back to "Any", not excluded, not paused; the row stays. */
export function clearRow(root: GroupDraft, rowId: string): GroupDraft {
  const row = findRow(root, rowId);
  const key = row === null ? null : fieldKey(row.value);
  const value: RowValue =
    key !== null
      ? emptyValue(key)
      : row?.value.field === "tag"
        ? { ...row.value, ids: [], all: false }
        : { field: null };
  return updateRow(root, rowId, { value, not: false, paused: false });
}

/** Gives a field to a row that has none yet (one made by Or…). */
export function chooseField(root: GroupDraft, rowId: string, key: FieldKey): GroupDraft {
  return updateRow(root, rowId, { value: emptyValue(key), not: false, paused: false });
}

/**
 * Add filter / + Or…: a row for `key` in `groupId`. On the root level a
 * field shows once: picking one that shows jumps to its row, and an unused
 * default hidden by a use elsewhere comes back as an added row.
 */
export function addField(
  root: GroupDraft,
  groupId: string,
  key: FieldKey,
): { root: GroupDraft; rowId: string } {
  if (groupId === root.id) {
    const hidden = hiddenRows(root);
    const same = root.rules.filter(
      (i): i is RowDraft => i.kind === "row" && fieldKey(i.value) === key,
    );
    const shown = same.find((r) => !hidden.has(r.id));
    if (shown !== undefined) return { root, rowId: shown.id };
    root = { ...root, rules: root.rules.filter((i) => !same.includes(i as RowDraft)) };
  }
  const row = newRow(emptyValue(key));
  return { root: insertInto(root, groupId, row), rowId: row.id };
}

/**
 * Or… on a row: in a group that already reads "or", an empty row after it;
 * else the row moves into a new "any" group with an empty row below it.
 * Returns `null` when that group would pass `MAX_LEVELS`.
 */
export function orRow(root: GroupDraft, rowId: string): { root: GroupDraft; rowId: string } | null {
  const parent = parentOf(root, rowId);
  const row = findRow(root, rowId);
  if (parent === null || row === null || !canOr(root, rowId)) return null;
  const empty = newRow();
  const next = inGroup(root, parent.id, (p) => ({
    ...p,
    rules: p.rules.flatMap((i) => {
      if (i.id !== rowId) return [i];
      return p.mode === "all"
        ? [newGroup("any", [{ ...row, placeholder: false }, empty])]
        : [i, empty];
    }),
  }));
  return { root: next, rowId: empty.id };
}

/** Puts a group's items in its place in the parent. */
export function ungroup(root: GroupDraft, groupId: string): GroupDraft {
  return mapItems(root, (i) => (i.kind === "group" && i.id === groupId ? i.rules : i));
}

function mergeValues(a: RowValue, b: RowValue): RowValue {
  const union = <T>(x: readonly T[], y: readonly T[]) => [...new Set([...x, ...y])];
  if (a.field === "genre" && b.field === "genre")
    return { ...a, genres: union(a.genres, b.genres) };
  if (a.field === "color" && b.field === "color")
    return { ...a, colours: union(a.colours, b.colours) };
  if (a.field === "playlist" && b.field === "playlist") return { ...a, ids: union(a.ids, b.ids) };
  if (a.field === "key" && b.field === "key") return { ...a, keys: union(a.keys, b.keys) };
  if (a.field === "tag" && b.field === "tag") return { ...a, ids: union(a.ids, b.ids) };
  return b;
}

/**
 * Quick paths ("Filter by tag X", "Show tracks not in this playlist"): fills
 * the root row of that field, adding to its values when it already filters
 * the same way, or adds the row.
 */
export function fillRow(
  root: GroupDraft,
  value: RowValue,
  not: boolean,
): { root: GroupDraft; rowId: string } {
  const key = fieldKey(value);
  const row = root.rules.find(
    (i): i is RowDraft => i.kind === "row" && key !== null && fieldKey(i.value) === key,
  );
  if (row === undefined) {
    const added = newRow(value, not);
    return { root: insertInto(root, root.id, added), rowId: added.id };
  }
  const merge = isRowSet(row) && row.not === not && !(row.value.field === "tag" && row.value.all);
  return {
    root: updateRow(root, row.id, {
      value: merge ? mergeValues(row.value, value) : value,
      not,
      paused: false,
    }),
    rowId: row.id,
  };
}

/**
 * A value picked in Add filter's search: on the root level it sets or
 * extends that field's row, like a quick path; in a nested group it is a new
 * row there.
 */
export function addValue(
  root: GroupDraft,
  groupId: string,
  value: RowValue,
): { root: GroupDraft; rowId: string } {
  if (groupId === root.id) return fillRow(root, value, false);
  const row = newRow(value);
  return { root: insertInto(root, groupId, row), rowId: row.id };
}

/** Tag ids gone from the tree leave their rows; rows of a deleted category go. */
export function pruneTags(
  root: GroupDraft,
  categories: ReadonlyMap<TagId, ReadonlySet<TagId>>,
): GroupDraft {
  const known = new Set([...categories.values()].flatMap((s) => [...s]));
  return mapItems(root, (i) => {
    if (i.kind !== "row" || i.value.field !== "tag") return i;
    const v = i.value;
    if (v.category !== null && !categories.has(v.category)) return null;
    const ids = v.ids.filter((id) => known.has(id));
    return ids.length === v.ids.length ? i : { ...i, value: { ...v, ids } };
  });
}
