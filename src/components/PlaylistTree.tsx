/**
 * The playlist tree in the sidebar. Click selects (and shows it in the
 * table), F2 / Enter renames in place, Delete asks first. Right-click for the
 * menu. Drop tracks on a playlist to add them; drag nodes to move them.
 */
import { type DragEvent, type KeyboardEvent, type MouseEvent, useCallback, useState } from "react";

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
import { ContextMenu, type MenuItem, type MenuState } from "./ContextMenu";

/** Indent per level and the row's own left padding, in px (rekordbox-like spacing). */
const INDENT = 18;
const PAD = 4;

export function PlaylistTree() {
  const nodes = usePlaylists((s) => s.nodes);
  const tree = usePlaylists((s) => s.tree);
  const error = usePlaylists((s) => s.error);
  const p = usePlaylists.getState;
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [drop, setDrop] = useState<{ id: number; zone: DropZone } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);

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

  function openMenu(e: MouseEvent, node: PlaylistNode | null) {
    e.preventDefault();
    if (node !== null) p().select(node.id);
    const items: MenuItem[] = [
      { label: "New playlist", onSelect: () => void p().create("playlist", node) },
      { label: "New folder", onSelect: () => void p().create("folder", node) },
    ];
    if (node !== null) {
      items.push(
        {
          label: "Rename",
          onSelect: () => p().dispatch({ type: "startRename", id: node.id, name: node.name }),
        },
        { label: "Delete…", onSelect: () => p().requestDelete(node) },
      );
    }
    setMenu({ x: e.clientX, y: e.clientY, items });
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
    <section className="playlists" aria-label="Playlists">
      {error !== null && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      <div
        className="tree"
        role="tree"
        aria-label="Playlist tree"
        tabIndex={0}
        onKeyDown={onKeyDown}
        onContextMenu={(e) => openMenu(e, null)}
      >
        <div
          role="treeitem"
          aria-level={1}
          aria-selected={tree.selectedId === null}
          className={`tree-row root${tree.selectedId === null ? " selected" : ""}`}
          onClick={() => p().select(null)}
        >
          <span className="tree-disclosure" />
          <CollectionIcon />
          <span className="tree-name">Collection</span>
        </div>
        <div
          role="treeitem"
          aria-level={1}
          aria-selected={false}
          aria-expanded={tree.playlistsOpen}
          className={`tree-row root${drop?.id === ROOT ? " drop-into" : ""}`}
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
              void p().mutate({ type: "move", id: moved, parentId: null, position: rootCount });
            }
          }}
        >
          <Disclosure open={tree.playlistsOpen} />
          <span className="tree-name">Playlists</span>
          <button
            type="button"
            className="tree-add"
            title="New playlist"
            aria-label="New playlist"
            onClick={(e) => {
              e.stopPropagation();
              if (!tree.playlistsOpen) p().dispatch({ type: "togglePlaylists" });
              void p().create("playlist", selected);
            }}
          >
            +
          </button>
        </div>
        {visible.map((node) => (
          <TreeRow
            key={node.id}
            node={node}
            selected={tree.selectedId === node.id}
            expanded={tree.expanded.has(node.id)}
            draft={tree.renaming?.id === node.id ? tree.renaming.draft : null}
            dropZone={drop?.id === node.id ? drop.zone : null}
            onContextMenu={(e) => {
              e.stopPropagation();
              openMenu(e, node);
            }}
            onDragOver={(e) => onDragOver(e, node)}
            onDragLeave={() => setDrop((d) => (d?.id === node.id ? null : d))}
            onDrop={(e) => onDrop(e, node)}
          />
        ))}
      </div>
      {menu !== null && <ContextMenu menu={menu} onClose={closeMenu} />}
    </section>
  );
}

/** Drop-target id of the "Playlists" root row (ids from SQLite are positive). */
const ROOT = -1;

function Disclosure({ open }: { open: boolean }) {
  return (
    <span className="tree-disclosure" aria-hidden="true">
      <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
        <path d={open ? "M1 2.5h8L5 8z" : "M2.5 1v8L8 5z"} fill="currentColor" />
      </svg>
    </span>
  );
}

function CollectionIcon() {
  return (
    <svg className="tree-icon" viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="8" cy="8" r="1.8" fill="currentColor" />
    </svg>
  );
}

function FolderIcon() {
  return (
    <svg className="tree-icon" viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <path d="M1 3.5h5l1.5 1.5H15v8.5H1z" fill="currentColor" />
    </svg>
  );
}

function PlaylistIcon() {
  return (
    <svg className="tree-icon" viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
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
  onContextMenu: (e: MouseEvent) => void;
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
      className={`tree-row${selected ? " selected" : ""}${dropZone ? ` drop-${dropZone}` : ""}`}
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
        <span className="tree-disclosure" />
      )}
      {node.isFolder ? <FolderIcon /> : <PlaylistIcon />}
      {draft === null ? (
        <span className="tree-name">{node.name}</span>
      ) : (
        <input
          className="tree-rename"
          aria-label="Name"
          value={draft}
          // biome-ignore lint/a11y/noAutofocus: rename-in-place starts typing at once.
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
