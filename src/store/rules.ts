/**
 * The Filters drawer's state (Zustand, D35): the one draft tree it edits,
 * the default fields, the smart playlist being edited and the row to focus
 * next. Every edit goes through the pure functions in `ruleTree.ts`; the
 * rows that filter reach the table as `library.rule` after a 50 ms pause
 * (like the search box), and only when the rule changed, so an unfinished
 * row never re-queries.
 *
 * Default fields come from settings (`ui.filterFields`, `null` = built in)
 * and the tag tree (the first My Tags category). Each edit puts back an
 * empty row for a default field the root level lost, unless the user
 * removed it from this drawer (`dismissed`, until the next fresh drawer).
 *
 * Saving a smart playlist sends `createSmart` / `setRule`; the rule itself is
 * validated by reks-core.
 */
import { create } from "zustand";

import {
  type PlaylistNode,
  playlistRule,
  type RuleNode,
  type TagId,
  type TagTree,
} from "../lib/ipc";
import { useLibrary } from "./library";
import { usePanels } from "./panels";
import { usePlaylists } from "./playlists";
import { reportAction } from "./report";
import {
  addField,
  addValue,
  chooseField,
  clearRow,
  defaultKeys,
  ensureDefaults,
  type FieldKey,
  fieldKey,
  fillRow,
  findRow,
  freshRoot,
  fromRule,
  type GroupDraft,
  type GroupMode,
  isBlank,
  orRow,
  pruneTags,
  type RowDraft,
  type RowValue,
  removeItem,
  setMode,
  toggleMode,
  toRule,
  ungroup,
  updateRow,
  visibleRowIds,
} from "./ruleTree";
import { useTags } from "./tags";
import { useToasts } from "./toasts";

/** Pause before an edit re-queries, ms (the same as the search box). */
export const RULE_DEBOUNCE_MS = 50;

/** A smart playlist whose rule is in the drawer. */
export interface EditingSmart {
  playlistId: number;
  name: string;
  /** The drawer's draft before the edit, put back when it ends. */
  before: GroupDraft;
}

/** The row that takes focus next; `open` also opens its value picker (a row just added). */
export interface RowFocus {
  id: string;
  open: boolean;
}

export interface RulesState {
  root: GroupDraft;
  editing: EditingSmart | null;
  focus: RowFocus | null;
  /** Settings `ui.filterFields`: the user's default fields; `null` = the built-in ones. */
  defaultFields: string[] | null;
  /** Default fields removed from this drawer; a fresh drawer brings them back. */
  dismissed: FieldKey[];
  /** The draft of the last result that had tracks ("Undo last change"). */
  lastMatched: GroupDraft | null;

  clearFocus(): void;
  focusRow(id: string): void;
  /** A row's value, Exclude or Pause. */
  updateRow(rowId: string, patch: Partial<Pick<RowDraft, "value" | "not" | "paused">>): void;
  /** × / Backspace on a set row: back to "Any"; the row stays. */
  clearRow(rowId: string): void;
  /** Removes a row or group; a default field removed stays out of this drawer. */
  remove(id: string): void;
  /** Add filter / + Or…: a row for `key` in `groupId`, or a jump to the one shown. */
  addField(groupId: string, key: FieldKey): void;
  /** Gives the empty row made by Or… its field. */
  chooseField(rowId: string, key: FieldKey): void;
  /** Add filter's search: a value for `groupId` (sets or extends the root row of its field). */
  addValue(groupId: string, value: RowValue): void;
  /** The empty row made by Or… takes a value found by the search. */
  chooseValue(rowId: string, value: RowValue): void;
  /** The root level's Add filter menu; `/` in the drawer opens it. */
  addOpen: boolean;
  setAddOpen(open: boolean): void;
  /** Or…: this row or another filter. */
  or(rowId: string): void;
  setMode(groupId: string, mode: GroupMode): void;
  /** A connector click: the level reads "or" instead of "and", or back. */
  toggleMode(groupId: string): void;
  ungroup(groupId: string): void;
  /** "Keep in default fields" on a row of field `key`. */
  keepDefault(key: FieldKey, keep: boolean): void;
  resetDefaults(): void;
  /** From settings at start-up. */
  setDefaultFields(fields: string[] | null): void;
  /** Quick path: fills or adds the root row of the value's field and opens the drawer. */
  addCondition(value: RowValue, not: boolean): void;
  /** A fresh drawer; returns what it held, for Undo. */
  clear(): GroupDraft;
  restore(root: GroupDraft): void;
  /** Puts back the draft of the last result that had tracks. */
  undoLastChange(): void;
  /** Loads a smart playlist's rule to edit it; the table shows the library as a live preview. */
  edit(playlistId: number, name: string, rule: RuleNode): void;
  /** Fetches a smart playlist's rule and starts editing it. */
  editSmart(node: PlaylistNode): Promise<void>;
  /**
   * Ends an edit without saving; the drawer gets its earlier draft back and,
   * unless `showList` is false (the user went elsewhere), the list is shown.
   */
  cancelEdit(showList?: boolean): void;
  /** `setRule` of the edited smart playlist. Resolves `true` when saved. */
  saveEdit(): Promise<boolean>;
  /** `createSmart` from the drawer's rule. Resolves `true` when saved. */
  saveAsSmart(name: string): Promise<boolean>;
}

let timer: ReturnType<typeof setTimeout> | undefined;

const same = (a: RuleNode | null, b: RuleNode | null) => JSON.stringify(a) === JSON.stringify(b);

function categoryIds(): TagId[] | null {
  return useTags.getState().tree?.categories.map((c) => c.id) ?? null;
}

function categoryOf(tag: TagId): TagId | undefined {
  return useTags.getState().tree?.categories.find((c) => c.tags.some((t) => t.id === tag))?.id;
}

export const useRules = create<RulesState>()((set, get) => {
  /** Default fields now, less the ones removed from this drawer. */
  function defaults(): FieldKey[] {
    const { defaultFields, dismissed } = get();
    return defaultKeys(defaultFields, categoryIds()).filter((k) => !dismissed.includes(k));
  }

  /** Applies `next` (default rows put back), then re-queries after the pause if the rule changed. */
  function apply(next: GroupDraft, focus: RowFocus | null = null) {
    set((s) => ({ root: ensureDefaults(next, defaults()), focus: focus ?? s.focus }));
    syncRule();
  }

  function syncRule(immediate = false) {
    clearTimeout(timer);
    const push = () => {
      const rule = toRule(get().root);
      if (!same(rule, useLibrary.getState().rule)) useLibrary.getState().setRule(rule);
    };
    if (immediate) push();
    else timer = setTimeout(push, RULE_DEBOUNCE_MS);
  }

  /** Default fields changed: a blank drawer starts over, else missing ones are added. */
  function defaultsChanged() {
    const { root, editing } = get();
    apply(isBlank(root) && editing === null ? freshRoot(defaults()) : root);
  }

  return {
    root: freshRoot(defaultKeys(null, null)),
    editing: null,
    focus: null,
    defaultFields: null,
    dismissed: [],
    lastMatched: null,

    clearFocus() {
      set({ focus: null });
    },
    focusRow(id) {
      set({ focus: { id, open: false } });
    },
    updateRow(rowId, patch) {
      apply(updateRow(get().root, rowId, patch));
    },
    clearRow(rowId) {
      apply(clearRow(get().root, rowId));
    },
    remove(id) {
      const root = get().root;
      const row = findRow(root, id);
      const key = row === null ? null : fieldKey(row.value);
      if (
        key !== null &&
        root.rules.some((i) => i.id === id) &&
        defaultKeys(get().defaultFields, categoryIds()).includes(key)
      ) {
        set((s) => ({ dismissed: [...s.dismissed, key] }));
      }
      const order = visibleRowIds(root);
      const at = order.indexOf(id);
      const next = order[at + 1] ?? order[at - 1] ?? null;
      apply(removeItem(root, id), next === null ? null : { id: next, open: false });
    },
    addField(groupId, key) {
      set((s) => ({ dismissed: s.dismissed.filter((k) => k !== key) }));
      const { root, rowId } = addField(get().root, groupId, key);
      apply(root, { id: rowId, open: true });
    },
    chooseField(rowId, key) {
      apply(chooseField(get().root, rowId, key), { id: rowId, open: true });
    },
    addValue(groupId, value) {
      const key = fieldKey(value);
      set((s) => ({ dismissed: s.dismissed.filter((k) => k !== key) }));
      const made = addValue(get().root, groupId, value);
      apply(made.root, { id: made.rowId, open: false });
    },
    chooseValue(rowId, value) {
      apply(updateRow(get().root, rowId, { value, not: false, paused: false }), {
        id: rowId,
        open: false,
      });
    },
    addOpen: false,
    setAddOpen(open) {
      set({ addOpen: open });
    },
    or(rowId) {
      const made = orRow(get().root, rowId);
      if (made !== null) apply(made.root, { id: made.rowId, open: true });
    },
    setMode(groupId, mode) {
      apply(setMode(get().root, groupId, mode));
    },
    toggleMode(groupId) {
      apply(toggleMode(get().root, groupId));
    },
    ungroup(groupId) {
      apply(ungroup(get().root, groupId));
    },
    keepDefault(key, keep) {
      const now = defaultKeys(get().defaultFields, categoryIds());
      const fields = keep ? [...now.filter((k) => k !== key), key] : now.filter((k) => k !== key);
      set((s) => ({
        defaultFields: fields,
        dismissed: keep ? s.dismissed.filter((k) => k !== key) : s.dismissed,
      }));
      apply(get().root);
    },
    resetDefaults() {
      set({ defaultFields: null, dismissed: [] });
      defaultsChanged();
    },
    setDefaultFields(fields) {
      set({ defaultFields: fields });
      defaultsChanged();
    },
    addCondition(value, not) {
      usePanels.setState({ filterBarOpen: true });
      const { root, rowId } = fillRow(get().root, value, not);
      apply(root, { id: rowId, open: false });
    },
    clear() {
      const before = get().root;
      set({ dismissed: [] });
      apply(freshRoot(defaults()));
      syncRule(true);
      return before;
    },
    restore(root) {
      apply(root);
      syncRule(true);
    },
    undoLastChange() {
      const last = get().lastMatched;
      if (last !== null) get().restore(last);
    },
    edit(playlistId, name, rule) {
      const before = get().editing?.before ?? get().root;
      usePanels.setState({ filterBarOpen: true });
      usePlaylists.getState().select(null);
      set({ editing: { playlistId, name, before } });
      apply(fromRule(rule, categoryOf));
      syncRule(true);
    },
    async editSmart(node) {
      try {
        get().edit(node.id, node.name, await playlistRule(node.id));
      } catch (e) {
        const msg = `Could not load the rules of “${node.name}”: ${e}`;
        useToasts.getState().show(msg, reportAction(msg));
      }
    },
    cancelEdit(showList = true) {
      const editing = get().editing;
      if (editing === null) return;
      set({ editing: null });
      apply(editing.before);
      syncRule(true);
      if (showList) usePlaylists.getState().select(editing.playlistId);
    },
    async saveEdit() {
      const { editing, root } = get();
      const rule = toRule(root);
      if (editing === null || rule === null) return false;
      const outcome = await usePlaylists
        .getState()
        .mutate({ type: "setRule", id: editing.playlistId, rule });
      if (outcome === null) return false;
      set({ editing: null });
      apply(editing.before);
      syncRule(true);
      usePlaylists.getState().select(editing.playlistId);
      useToasts.getState().show(`Saved the rules of “${editing.name}”`);
      return true;
    },
    async saveAsSmart(name) {
      const rule = toRule(get().root);
      if (rule === null) return false;
      const playlists = usePlaylists.getState();
      const outcome = await playlists.mutate({
        type: "createSmart",
        parentId: null,
        name: name.trim(),
        rule,
      });
      if (outcome?.type !== "done" || outcome.createdId === null) return false;
      // The rule now lives in the playlist; showing it ANDed with the same rule would only confuse.
      get().clear();
      playlists.select(outcome.createdId);
      useToasts.getState().show(`Saved smart playlist “${name.trim()}”`);
      return true;
    },
  };
});

/** Empties the drawer (the shown playlist stays), with one Undo toast. */
export function clearAllFilters(): void {
  const before = useRules.getState().clear();
  useToasts.getState().show("Cleared all filters", {
    label: "Undo",
    run() {
      useRules.getState().restore(before);
    },
  });
}

// Leaving a smart playlist's edit for another list ends it; the Collection is the edit's own view.
usePlaylists.subscribe((s, prev) => {
  const editing = useRules.getState().editing;
  const id = s.tree.selectedId;
  if (editing === null || id === prev.tree.selectedId || id === null) return;
  if (id !== editing.playlistId) useRules.getState().cancelEdit(false);
});

// Tags deleted leave the rows that named them; a new first category becomes a default field.
const tagShape = (tree: TagTree | null) =>
  tree?.categories.map((c) => `${c.id}:${c.tags.map((t) => t.id)}`).join(" ");

useTags.subscribe((s, prev) => {
  if (s.tree === null || tagShape(s.tree) === tagShape(prev.tree)) return;
  const categories = new Map(
    s.tree.categories.map((c) => [c.id, new Set(c.tags.map((t) => t.id))] as const),
  );
  const rules = useRules.getState();
  useRules.setState({ root: pruneTags(rules.root, categories) });
  rules.setDefaultFields(rules.defaultFields);
});

// Remembers the draft whenever its result has tracks, for "Undo last change".
useLibrary.subscribe((s, prev) => {
  const landed = s.pagesGeneration === s.generation && s.pagesGeneration !== prev.pagesGeneration;
  if (!landed || s.total === 0 || !s.filtersEnabled) return;
  const { root } = useRules.getState();
  if (same(toRule(root), s.rule)) useRules.setState({ lastMatched: root });
});
