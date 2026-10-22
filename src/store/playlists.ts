/**
 * Playlists (Zustand): the tree from `playlists_get`, its view state
 * (`playlistTree.ts`), and every edit through `playlists_mutate`. After an
 * edit the tree, the facets and the track table are reloaded. Ordering and
 * duplicate rules live in reks-core; this only forwards what the user did and
 * shows what came back.
 */
import { create } from "zustand";

import type { DropZone } from "../lib/dnd";
import {
  type DuplicateMode,
  type PlaylistNode,
  type PlaylistOp,
  type PlaylistOutcome,
  playlistsGet,
  playlistsMutate,
} from "../lib/ipc";
import { useDeviceBrowser } from "./deviceBrowser";
import { useHistory } from "./history";
import { useLibrary } from "./library";
import { allLibraries, usePanes } from "./panes";
import {
  ancestors,
  dropTarget,
  initialTree,
  type TreeAction,
  type TreeState,
  treeReducer,
} from "./playlistTree";

type AddTracksOp = Extract<PlaylistOp, { type: "addTracks" }>;
type DuplicatesOutcome = Extract<PlaylistOutcome, { type: "duplicates" }>;

/** Label a new divider starts with, before the user renames it in place. */
export const NEW_DIVIDER_LABEL = "New divider";

/** A divider being renamed in place in the track table. */
export interface DividerRename {
  playlistId: number;
  position: number;
  /** The label before the edit. */
  label: string;
  draft: string;
}

/** An "add tracks" that found duplicates and waits for the user's choice. */
export interface PendingDuplicates {
  op: AddTracksOp;
  outcome: DuplicatesOutcome;
  playlistName: string;
}

export interface PlaylistsState {
  nodes: PlaylistNode[];
  tree: TreeState;
  pendingDuplicates: PendingDuplicates | null;
  /** Node waiting for the delete confirmation. */
  confirmDelete: PlaylistNode | null;
  dividerRename: DividerRename | null;
  error: string | null;

  dispatch(action: TreeAction): void;
  load(): Promise<void>;
  /** Sends `op`, then reloads. `null` when it failed (the error is in `error`). */
  mutate(op: PlaylistOp): Promise<PlaylistOutcome | null>;
  /** Selects a node (or the collection) and shows it in the track table (closing a session). */
  select(id: number | null): void;
  /** Shift+click: shows a node in a split pane under the track table (closing a session). */
  openInPane(id: number): void;
  /** The main list's close button: the first split pane's playlist moves up into it. */
  closeMainList(): void;
  /** New playlist or folder inside `near` (a folder) or next to it, then renames it. */
  create(kind: "playlist" | "folder", near: PlaylistNode | null): Promise<void>;
  commitRename(): Promise<void>;
  requestDelete(node: PlaylistNode): void;
  resolveDelete(confirmed: boolean): Promise<void>;
  moveNode(id: number, target: PlaylistNode, zone: DropZone): Promise<void>;
  /** Always asks about duplicates first. */
  addTracks(playlistId: number, trackIds: number[], before?: number | null): Promise<void>;
  /** The duplicate dialog's answer; `null` = cancel. */
  resolveDuplicates(mode: Exclude<DuplicateMode, "ask"> | null): Promise<void>;
  removeEntries(playlistId: number, positions: number[]): Promise<void>;
  reorderEntries(playlistId: number, positions: number[], before: number): Promise<void>;
  /** Inserts a divider before entry `position`, then renames it in place. */
  insertDivider(playlistId: number, position: number): Promise<void>;
  startDividerRename(playlistId: number, position: number, label: string): void;
  editDividerDraft(draft: string): void;
  commitDividerRename(): Promise<void>;
  cancelDividerRename(): void;
}

export const usePlaylists = create<PlaylistsState>()((set, get) => {
  function byId(id: number | null): PlaylistNode | undefined {
    return id === null ? undefined : get().nodes.find((n) => n.id === id);
  }

  async function afterEdit(): Promise<void> {
    await get().load();
    const lib = useLibrary.getState();
    void lib.loadFacets();
    const shown = lib.filters.playlistId;
    if (shown !== null && byId(shown) === undefined) lib.showPlaylist(null, true);
    else lib.refresh(true);
    const panes = usePanes.getState();
    for (const pane of panes.panes) {
      if (byId(pane.playlistId) === undefined) panes.close(pane.key);
      else pane.store.getState().refresh(true);
    }
  }

  return {
    nodes: [],
    tree: initialTree,
    pendingDuplicates: null,
    confirmDelete: null,
    dividerRename: null,
    error: null,

    dispatch(action) {
      set((s) => ({ tree: treeReducer(s.tree, action) }));
    },
    async load() {
      try {
        const { nodes } = await playlistsGet();
        set((s) => ({ nodes, tree: treeReducer(s.tree, { type: "treeLoaded", nodes }) }));
      } catch (e) {
        set({ error: String(e) });
      }
    },
    async mutate(op) {
      try {
        const outcome = await playlistsMutate(op);
        set({ error: null });
        if (outcome.type === "done") await afterEdit();
        return outcome;
      } catch (e) {
        set({ error: String(e) });
        return null;
      }
    },
    select(id) {
      useHistory.getState().close();
      useDeviceBrowser.getState().close();
      get().dispatch({ type: "select", id });
      const node = byId(id);
      usePanes.getState().closePlaylist(node?.id ?? null);
      useLibrary.getState().showPlaylist(node?.id ?? null, node?.isFolder ?? true);
    },
    openInPane(id) {
      const node = byId(id);
      if (node === undefined) return;
      useHistory.getState().close();
      usePanes.getState().open(node.id, node.isFolder);
    },
    closeMainList() {
      const first = usePanes.getState().panes[0];
      if (first !== undefined) get().select(first.playlistId);
    },
    async create(kind, near) {
      let parentId: number | null = null;
      let position: number | null = null;
      if (near?.isFolder) parentId = near.id;
      else if (near) {
        parentId = near.parentId;
        position = near.position + 1;
      }
      const name = kind === "folder" ? "New folder" : "New playlist";
      const outcome = await get().mutate({
        type: kind === "folder" ? "createFolder" : "createPlaylist",
        parentId,
        name,
        position,
      });
      if (outcome?.type !== "done" || outcome.createdId === null) return;
      const { dispatch } = get();
      for (const id of ancestors(get().nodes, outcome.createdId)) dispatch({ type: "expand", id });
      dispatch({ type: "startRename", id: outcome.createdId, name });
    },
    async commitRename() {
      const r = get().tree.renaming;
      if (r === null) return;
      get().dispatch({ type: "endRename" });
      if (byId(r.id)?.name === r.draft) return;
      await get().mutate({ type: "rename", id: r.id, name: r.draft });
    },
    requestDelete(node) {
      set({ confirmDelete: node });
    },
    async resolveDelete(confirmed) {
      const node = get().confirmDelete;
      set({ confirmDelete: null });
      if (confirmed && node !== null) await get().mutate({ type: "delete", id: node.id });
    },
    async moveNode(id, target, zone) {
      if (id === target.id) return;
      const { parentId, position } = dropTarget(get().nodes, target, zone);
      await get().mutate({ type: "move", id, parentId, position });
    },
    async addTracks(playlistId, trackIds, before = null) {
      // A drag of dividers only carries no tracks.
      if (trackIds.length === 0) return;
      const op: AddTracksOp = {
        type: "addTracks",
        playlistId,
        trackIds,
        before,
        duplicates: "ask",
      };
      const outcome = await get().mutate(op);
      if (outcome?.type === "duplicates") {
        set({
          pendingDuplicates: { op, outcome, playlistName: byId(playlistId)?.name ?? "" },
        });
      }
    },
    async resolveDuplicates(mode) {
      const pending = get().pendingDuplicates;
      set({ pendingDuplicates: null });
      if (pending === null || mode === null) return;
      await get().mutate({ ...pending.op, duplicates: mode });
    },
    async removeEntries(playlistId, positions) {
      if (positions.length === 0) return;
      await get().mutate({ type: "removeEntries", playlistId, positions });
    },
    async reorderEntries(playlistId, positions, before) {
      if (positions.length === 0) return;
      await get().mutate({ type: "reorderEntries", playlistId, positions, before });
    },
    async insertDivider(playlistId, position) {
      const label = NEW_DIVIDER_LABEL;
      const outcome = await get().mutate({ type: "insertDivider", playlistId, position, label });
      if (outcome?.type !== "done") return;
      // The reloaded table showing it puts the new divider at table row `position`.
      for (const lib of allLibraries())
        if (lib.getState().filters.playlistId === playlistId)
          lib.getState().select({ type: "click", index: position });
      get().startDividerRename(playlistId, position, label);
    },
    startDividerRename(playlistId, position, label) {
      set({ dividerRename: { playlistId, position, label, draft: label } });
    },
    editDividerDraft(draft) {
      set((s) =>
        s.dividerRename === null ? {} : { dividerRename: { ...s.dividerRename, draft } },
      );
    },
    async commitDividerRename() {
      const r = get().dividerRename;
      if (r === null) return;
      set({ dividerRename: null });
      if (r.draft === r.label) return;
      await get().mutate({
        type: "renameDivider",
        playlistId: r.playlistId,
        position: r.position,
        label: r.draft,
      });
    },
    cancelDividerRename() {
      set({ dividerRename: null });
    },
  };
});

/** The playlist (not folder) the track table shows, if any. */
export function shownPlaylist(nodes: readonly PlaylistNode[], playlistId: number | null) {
  const node = nodes.find((n) => n.id === playlistId);
  return node !== undefined && !node.isFolder ? node : null;
}
