/**
 * The rule builder's state (Zustand): the draft tree, whether the panel is
 * open, the smart playlist being edited, and the row to focus next. Every
 * edit goes through the pure functions in `ruleTree.ts`; the finished rows
 * reach the table as `library.rule` after a 50 ms pause (like the search box),
 * and only when they changed, so an unfinished row never re-queries.
 *
 * Saving a smart playlist sends `createSmart` / `setRule`; the rule itself is
 * validated by reks-core.
 */
import { create } from "zustand";

import { type PlaylistNode, playlistRule, type RuleNode } from "../lib/ipc";
import { type FilterState, useLibrary } from "./library";
import { usePanels } from "./panels";
import { usePlaylists } from "./playlists";
import {
  addAfter,
  addItem,
  canAddGroup,
  defaultValue,
  emptyRoot,
  fromRule,
  type GroupDraft,
  type GroupMode,
  isBlank,
  neighbourRow,
  newGroup,
  newRow,
  type RowValue,
  type RuleField,
  removeItem,
  setMode,
  toRule,
  updateRow,
} from "./ruleTree";
import { useToasts } from "./toasts";

/** Pause before an edit re-queries, ms (the same as the search box). */
export const RULE_DEBOUNCE_MS = 50;

/** A smart playlist whose rule is in the builder. */
export interface EditingSmart {
  playlistId: number;
  name: string;
  /** The builder's draft before the edit, put back when it ends. */
  before: GroupDraft;
}

export interface RulesState {
  root: GroupDraft;
  open: boolean;
  editing: EditingSmart | null;
  /** The row whose field Select takes focus next; cleared once it has. */
  focusId: string | null;

  setOpen(open: boolean): void;
  toggleOpen(): void;
  clearFocus(): void;
  addRow(groupId: string): void;
  addRowAfter(rowId: string): void;
  addGroup(groupId: string): void;
  remove(id: string): void;
  setMode(groupId: string, mode: GroupMode): void;
  setNot(rowId: string, not: boolean): void;
  /** Picks a field (and resets the value) or, with `null`, empties the row. */
  setField(rowId: string, field: RuleField | null): void;
  setValue(rowId: string, value: RowValue): void;
  /** Quick path: a finished row (reusing the empty first row), builder opened. */
  addCondition(value: RowValue, not: boolean): void;
  /** Empties the builder; returns what it held, for Undo. */
  clear(): GroupDraft;
  restore(root: GroupDraft): void;
  /** Loads a smart playlist's rule to edit it; the table shows the library as a live preview. */
  edit(playlistId: number, name: string, rule: RuleNode): void;
  /** Fetches a smart playlist's rule and starts editing it. */
  editSmart(node: PlaylistNode): Promise<void>;
  /**
   * Ends an edit without saving; the builder gets its earlier draft back and,
   * unless `showList` is false (the user went elsewhere), the list is shown.
   */
  cancelEdit(showList?: boolean): void;
  /** `setRule` of the edited smart playlist. Resolves `true` when saved. */
  saveEdit(): Promise<boolean>;
  /** `createSmart` from the builder's rule. Resolves `true` when saved. */
  saveAsSmart(name: string): Promise<boolean>;
}

let timer: ReturnType<typeof setTimeout> | undefined;

const same = (a: RuleNode | null, b: RuleNode | null) => JSON.stringify(a) === JSON.stringify(b);

export const useRules = create<RulesState>()((set, get) => {
  /** Applies `next`, then re-queries after the pause if the finished rows changed. */
  function apply(next: GroupDraft, focusId: string | null = null) {
    set((s) => ({ root: next, focusId: focusId ?? s.focusId }));
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

  return {
    root: emptyRoot(),
    open: false,
    editing: null,
    focusId: null,

    setOpen(open) {
      set({ open });
    },
    toggleOpen() {
      set((s) => ({ open: !s.open }));
    },
    clearFocus() {
      set({ focusId: null });
    },
    addRow(groupId) {
      const row = newRow();
      apply(addItem(get().root, groupId, row), row.id);
    },
    addRowAfter(rowId) {
      const row = newRow();
      apply(addAfter(get().root, rowId, row), row.id);
    },
    addGroup(groupId) {
      const root = get().root;
      if (!canAddGroup(root, groupId)) return;
      const row = newRow();
      apply(addItem(root, groupId, newGroup("all", [row])), row.id);
    },
    remove(id) {
      const root = get().root;
      apply(removeItem(root, id), neighbourRow(root, id));
    },
    setMode(groupId, mode) {
      apply(setMode(get().root, groupId, mode));
    },
    setNot(rowId, not) {
      apply(updateRow(get().root, rowId, { not }));
    },
    setField(rowId, field) {
      apply(
        updateRow(get().root, rowId, {
          value: field === null ? { field: null } : defaultValue(field),
        }),
      );
    },
    setValue(rowId, value) {
      apply(updateRow(get().root, rowId, { value }));
    },
    addCondition(value, not) {
      const root = get().root;
      usePanels.setState({ filterBarOpen: true });
      set({ open: true });
      const row = newRow(value, not);
      // An untouched builder starts from the new row instead of keeping an empty one beside it.
      apply(isBlank(root) ? { ...root, rules: [row] } : addItem(root, root.id, row));
    },
    clear() {
      const before = get().root;
      apply(emptyRoot());
      syncRule(true);
      return before;
    },
    restore(root) {
      apply(root);
      syncRule(true);
    },
    edit(playlistId, name, rule) {
      const before = get().editing?.before ?? get().root;
      usePanels.setState({ filterBarOpen: true });
      usePlaylists.getState().select(null);
      set({ editing: { playlistId, name, before }, open: true });
      apply(fromRule(rule));
      syncRule(true);
    },
    async editSmart(node) {
      try {
        get().edit(node.id, node.name, await playlistRule(node.id));
      } catch (e) {
        useToasts.getState().show(`Could not load the rules of “${node.name}”: ${e}`);
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
      set({ open: false });
      playlists.select(outcome.createdId);
      useToasts.getState().show(`Saved smart playlist “${name.trim()}”`);
      return true;
    },
  };
});

/** What "Clear all" undoes: the bar's values and the builder's draft. */
interface Cleared {
  filters: FilterState;
  root: GroupDraft;
}

/**
 * Clears the filter bar and the rule builder together, with one Undo toast.
 * The shown playlist stays.
 */
export function clearAllFilters(): void {
  const lib = useLibrary.getState();
  const before: Cleared = { filters: lib.filters, root: useRules.getState().root };
  lib.clearFilters();
  useRules.getState().clear();
  useToasts.getState().show("Cleared all filters", {
    label: "Undo",
    run() {
      const { playlistId: _p, ...values } = before.filters;
      useLibrary.getState().setFilters(values);
      useRules.getState().restore(before.root);
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
