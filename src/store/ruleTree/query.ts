/** Reading a draft: which rows show, where things are, what counts. */
import {
  type FieldKey,
  fieldKey,
  type GroupDraft,
  type ItemDraft,
  MAX_LEVELS,
  type RowDraft,
} from "./model";
import { isRowActive, isRowSet } from "./toRule";

export function rows(item: ItemDraft): RowDraft[] {
  return item.kind === "row" ? [item] : item.rules.flatMap(rows);
}

/** Rows that filter (what the Filters badge counts). */
export function activeRowCount(root: GroupDraft): number {
  return rows(root).filter(isRowActive).length;
}

/** Nothing set anywhere and no groups: Clear all has nothing to do. */
export function isBlank(root: GroupDraft): boolean {
  return root.rules.every((i) => i.kind === "row" && !isRowSet(i));
}

// ---- reading the draft -----------------------------------------------------

export function isUnusedDefault(item: ItemDraft): item is RowDraft {
  return item.kind === "row" && item.placeholder && !isRowSet(item);
}

/**
 * Root rows that don't show: an unused default whose field is used
 * elsewhere in the query (it comes back when that use goes), or a second
 * unused default of the same field.
 */
export function hiddenRows(root: GroupDraft): Set<string> {
  const used = new Map<FieldKey, number>();
  const unused: RowDraft[] = [];
  for (const r of rows(root)) {
    const key = fieldKey(r.value);
    if (key === null) continue;
    if (root.rules.includes(r) && isUnusedDefault(r)) unused.push(r);
    else used.set(key, (used.get(key) ?? 0) + 1);
  }
  const hidden = new Set<string>();
  const seen = new Set<FieldKey>();
  for (const r of unused) {
    const key = fieldKey(r.value) as FieldKey;
    if (used.has(key) || seen.has(key)) hidden.add(r.id);
    seen.add(key);
  }
  return hidden;
}

/** Ids of the rows that show, in reading order (↑ / ↓ move along it). */
export function visibleRowIds(root: GroupDraft): string[] {
  const hidden = hiddenRows(root);
  return rows(root)
    .filter((r) => !hidden.has(r.id))
    .map((r) => r.id);
}

/** Levels from the root down to the item with `id` (the root group is level 1). */
export function levelOf(root: GroupDraft, id: string): number | null {
  if (root.id === id) return 1;
  for (const i of root.rules) {
    if (i.id === id) return 2;
    if (i.kind === "group") {
      const inner = levelOf(i, id);
      if (inner !== null) return inner + 1;
    }
  }
  return null;
}

export function findGroup(root: GroupDraft, id: string): GroupDraft | null {
  if (root.id === id) return root;
  for (const i of root.rules) {
    if (i.kind !== "group") continue;
    const hit = findGroup(i, id);
    if (hit !== null) return hit;
  }
  return null;
}

export function findRow(root: GroupDraft, id: string): RowDraft | null {
  return rows(root).find((r) => r.id === id) ?? null;
}

/** The group directly holding the item `id`. */
export function parentOf(root: GroupDraft, id: string): GroupDraft | null {
  if (root.rules.some((i) => i.id === id)) return root;
  for (const i of root.rules) {
    if (i.kind !== "group") continue;
    const hit = parentOf(i, id);
    if (hit !== null) return hit;
  }
  return null;
}

/** Field keys of the rows that show directly in `group` (the field chooser greys them "shown"). */
export function shownKeys(root: GroupDraft, group: GroupDraft): Set<FieldKey> {
  const hidden = group === root ? hiddenRows(root) : new Set<string>();
  return new Set(
    group.rules.flatMap((i) => {
      const key = i.kind === "row" && !hidden.has(i.id) ? fieldKey(i.value) : null;
      return key === null ? [] : [key];
    }),
  );
}

/** Or… on the row can go one level deeper, or its group already reads "or". */
export function canOr(root: GroupDraft, rowId: string): boolean {
  const parent = parentOf(root, rowId);
  if (parent === null) return false;
  return parent.mode !== "all" || (levelOf(root, parent.id) ?? MAX_LEVELS) < MAX_LEVELS;
}
