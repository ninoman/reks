/**
 * The playlist tree in the sidebar. Click selects (and shows it in the
 * table), F2 / Enter renames in place, Delete asks first. Right-click for the
 * menu. Drop tracks on a playlist to add them; drag nodes to move them.
 */
import { type DragEvent, type KeyboardEvent, useRef, useState } from "react";

import {
  type DropZone,
  hasType,
  NODE_MIME,
  readNode,
  readTracks,
  TRACKS_MIME,
  zoneAt,
} from "../lib/dnd";
import type { PlaylistNode } from "../lib/ipc";
import { usePlaylists } from "../store/playlists";
import { visibleNodes } from "../store/playlistTree";
import { PlusIcon } from "./icons";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "./ui/ContextMenu";
import { focusRingInset } from "./ui/control";
import { IconButton } from "./ui/IconButton";
import { Input } from "./ui/Input";
import { ScrollArea } from "./ui/ScrollArea";

/** One tree row; state comes in as data attributes (cheap, no class building). */
const ROW = [
  "flex h-(--row-h) cursor-default items-center gap-1.5 whitespace-nowrap pr-2 pl-1 text-md text-fg",
  "data-[root]:text-fg-muted data-[selected]:bg-selected data-[selected]:text-selected-fg",
  "data-[drop=into]:shadow-[inset_0_0_0_2px_var(--color-accent)]",
  "data-[drop=before]:shadow-[inset_0_2px_0_var(--color-accent)]",
  "data-[drop=after]:shadow-[inset_0_-2px_0_var(--color-accent)]",
].join(" ");
const DISCLOSURE = "inline-flex w-3.5 flex-none justify-center text-fg-muted";
const ICON = "flex-none text-fg-muted";
const NAME = "flex-1 overflow-hidden text-ellipsis";

/** Indent per level and the row's own left padding, in px (rekordbox-like spacing). */
const INDENT = 18;
const PAD = 4;

export function PlaylistTree() {
  const nodes = usePlaylists((s) => s.nodes);
  const tree = usePlaylists((s) => s.tree);
  const error = usePlaylists((s) => s.error);
  const p = usePlaylists.getState;
  /** The node the context menu is for (`null`: the tree itself). */
  const [menuNode, setMenuNode] = useState<PlaylistNode | null>(null);
  /** Set by a row's right-click, read by the tree's (which opens the menu). */
  const hit = useRef<PlaylistNode | null>(null);
  const [drop, setDrop] = useState<{ id: number; zone: DropZone } | null>(null);

  const visible = tree.playlistsOpen ? visibleNodes(nodes, tree.expanded) : [];
  const selected = nodes.find((n) => n.id === tree.selectedId) ?? null;

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (tree.renaming !== null) return;
    const i = selected === null ? -1 : visible.findIndex((n) => n.id === selected.id);
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        const next = Math.min(
          visible.length - 1,
          Math.max(-1, i + (e.key === "ArrowDown" ? 1 : -1)),
        );
        p().select(next === -1 ? null : (visible[next]?.id ?? null));
        break;
      }
      case "ArrowRight":
        if (selected?.isFolder) p().dispatch({ type: "expand", id: selected.id });
        break;
      case "ArrowLeft":
        if (selected?.isFolder) p().dispatch({ type: "collapse", id: selected.id });
        break;
      case "F2":
      case "Enter":
        if (selected !== null) {
          e.preventDefault();
          p().dispatch({ type: "startRename", id: selected.id, name: selected.name });
        }
        break;
      case "Delete":
      case "Backspace":
        if (selected !== null) {
          e.preventDefault();
          p().requestDelete(selected);
        }
        break;
    }
  }

  function onTreeContextMenu() {
    const node = hit.current;
    hit.current = null;
    if (node !== null) p().select(node.id);
    setMenuNode(node);
  }

  function onDragOver(e: DragEvent<HTMLElement>, node: PlaylistNode) {
    const dt = e.dataTransfer;
    if (hasType(dt, TRACKS_MIME)) {
      if (node.isFolder) return;
      e.preventDefault();
      dt.dropEffect = "copy";
      setDrop({ id: node.id, zone: "into" });
    } else if (hasType(dt, NODE_MIME)) {
      e.preventDefault();
      dt.dropEffect = "move";
      const rect = e.currentTarget.getBoundingClientRect();
      setDrop({ id: node.id, zone: zoneAt(e.clientY - rect.top, rect.height, node.isFolder) });
    }
  }

  function onDrop(e: DragEvent<HTMLElement>, node: PlaylistNode) {
    e.preventDefault();
    const zone = drop?.id === node.id ? drop.zone : "into";
    setDrop(null);
    const tracks = readTracks(e.dataTransfer);
    if (tracks !== null) {
      if (!node.isFolder) void p().addTracks(node.id, tracks.trackIds);
      return;
    }
    const moved = readNode(e.dataTransfer);
    if (moved !== null) void p().moveNode(moved, node, zone);
  }

  const rootCount = nodes.filter((n) => n.parentId === null).length;

  return (
    <section className="flex min-h-0 flex-1 flex-col" aria-label="Playlists">
      {error !== null && (
        <div className="mx-2 my-1.5 text-danger select-text" role="alert">
          {error}
        </div>
      )}
      <ContextMenu modal={false}>
        <ContextMenuTrigger asChild>
          <ScrollArea className="min-h-0 flex-1">
            <div
              className={`flex min-h-full flex-col py-1 ${focusRingInset}`}
              role="tree"
              aria-label="Playlist tree"
              tabIndex={0}
              onKeyDown={onKeyDown}
              onContextMenu={onTreeContextMenu}
            >
              <div
                role="treeitem"
                aria-level={1}
                aria-selected={tree.selectedId === null}
                className={ROW}
                data-root=""
                data-selected={tree.selectedId === null || undefined}
                onClick={() => p().select(null)}
              >
                <span className={DISCLOSURE} />
                <CollectionIcon />
                <span className={NAME}>Collection</span>
              </div>
              <div
                role="treeitem"
                aria-level={1}
                aria-selected={false}
                aria-expanded={tree.playlistsOpen}
                className={ROW}
                data-root=""
                data-drop={drop?.id === ROOT ? "into" : undefined}
                onClick={() => p().dispatch({ type: "togglePlaylists" })}
                onDragOver={(e) => {
                  if (!hasType(e.dataTransfer, NODE_MIME)) return;
                  e.preventDefault();
                  setDrop({ id: ROOT, zone: "into" });
                }}
                onDragLeave={() => setDrop((d) => (d?.id === ROOT ? null : d))}
                onDrop={(e) => {
                  e.preventDefault();
                  setDrop(null);
                  const moved = readNode(e.dataTransfer);
                  if (moved !== null) {
                    void p().mutate({
                      type: "move",
                      id: moved,
                      parentId: null,
                      position: rootCount,
                    });
                  }
                }}
              >
                <Disclosure open={tree.playlistsOpen} />
                <span className={NAME}>Playlists</span>
                <IconButton
                  size="xs"
                  shape="round"
                  variant="outline"
                  title="New playlist"
                  aria-label="New playlist"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (!tree.playlistsOpen) p().dispatch({ type: "togglePlaylists" });
                    void p().create("playlist", selected);
                  }}
                >
                  <PlusIcon />
                </IconButton>
              </div>
              {visible.map((node) => (
                <TreeRow
                  key={node.id}
                  node={node}
                  selected={tree.selectedId === node.id}
                  expanded={tree.expanded.has(node.id)}
                  draft={tree.renaming?.id === node.id ? tree.renaming.draft : null}
                  dropZone={drop?.id === node.id ? drop.zone : null}
                  onContextMenu={() => {
                    hit.current = node;
                  }}
                  onDragOver={(e) => onDragOver(e, node)}
                  onDragLeave={() => setDrop((d) => (d?.id === node.id ? null : d))}
                  onDrop={(e) => onDrop(e, node)}
                />
              ))}
            </div>
          </ScrollArea>
        </ContextMenuTrigger>
        {/* Focus stays where the choice put it (rename starts an input there). */}
        <ContextMenuContent onCloseAutoFocus={(e) => e.preventDefault()}>
          <ContextMenuItem onSelect={() => void p().create("playlist", menuNode)}>
            New playlist
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => void p().create("folder", menuNode)}>
            New folder
          </ContextMenuItem>
          {menuNode !== null && (
            <>
              <ContextMenuItem
                onSelect={() =>
                  p().dispatch({ type: "startRename", id: menuNode.id, name: menuNode.name })
                }
              >
                Rename
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => p().requestDelete(menuNode)}>
                Delete…
              </ContextMenuItem>
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>
    </section>
  );
}

/** Drop-target id of the "Playlists" root row (ids from SQLite are positive). */
const ROOT = -1;

function Disclosure({ open }: { open: boolean }) {
  return (
    <span className={DISCLOSURE} aria-hidden="true">
      <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
        <path d={open ? "M1 2.5h8L5 8z" : "M2.5 1v8L8 5z"} fill="currentColor" />
      </svg>
    </span>
  );
}

function CollectionIcon() {
  return (
    <svg className={ICON} viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="8" cy="8" r="1.8" fill="currentColor" />
    </svg>
  );
}

function FolderIcon() {
  return (
    <svg className={ICON} viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <path d="M1 3.5h5l1.5 1.5H15v8.5H1z" fill="currentColor" />
    </svg>
  );
}

function PlaylistIcon() {
  return (
    <svg className={ICON} viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <rect
        x="2"
        y="1.5"
        width="12"
        height="13"
        rx="1"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
      />
      <path d="M4.5 5h7M4.5 8h7M4.5 11h7" stroke="currentColor" strokeWidth="1.3" />
    </svg>
  );
}

function TreeRow({
  node,
  selected,
  expanded,
  draft,
  dropZone,
  onContextMenu,
  onDragOver,
  onDragLeave,
  onDrop,
}: {
  node: PlaylistNode;
  selected: boolean;
  expanded: boolean;
  draft: string | null;
  dropZone: DropZone | null;
  onContextMenu: () => void;
  onDragOver: (e: DragEvent<HTMLElement>) => void;
  onDragLeave: () => void;
  onDrop: (e: DragEvent<HTMLElement>) => void;
}) {
  const p = usePlaylists.getState;
  return (
    <div
      role="treeitem"
      aria-level={node.depth + 2}
      aria-selected={selected}
      aria-expanded={node.isFolder ? expanded : undefined}
      className={ROW}
      data-selected={selected || undefined}
      data-drop={dropZone ?? undefined}
      style={{ paddingLeft: PAD + (node.depth + 1) * INDENT }}
      title={node.isFolder ? undefined : `${node.entries} tracks`}
      draggable={draft === null}
      onClick={() => p().select(node.id)}
      onDoubleClick={() => node.isFolder && p().dispatch({ type: "toggle", id: node.id })}
      onContextMenu={onContextMenu}
      onDragStart={(e) => {
        e.dataTransfer.setData(NODE_MIME, String(node.id));
        e.dataTransfer.effectAllowed = "move";
      }}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
    >
      {node.isFolder ? (
        <span
          onClick={(e) => {
            e.stopPropagation();
            p().dispatch({ type: "toggle", id: node.id });
          }}
        >
          <Disclosure open={expanded} />
        </span>
      ) : (
        <span className={DISCLOSURE} />
      )}
      {node.isFolder ? <FolderIcon /> : <PlaylistIcon />}
      {draft === null ? (
        <span className={NAME}>{node.name}</span>
      ) : (
        <Input
          size="xs"
          className="flex-1"
          aria-label="Name"
          value={draft}
          // Rename-in-place starts typing at once.
          autoFocus
          onFocus={(e) => e.currentTarget.select()}
          onClick={(e) => e.stopPropagation()}
          onChange={(e) => p().dispatch({ type: "editDraft", draft: e.target.value })}
          onBlur={() => void p().commitRename()}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") void p().commitRename();
            else if (e.key === "Escape") p().dispatch({ type: "endRename" });
          }}
        />
      )}
    </div>
  );
}
