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

const INDENT = 14;

export function PlaylistTree() {
  const nodes = usePlaylists((s) => s.nodes);
  const tree = usePlaylists((s) => s.tree);
  const error = usePlaylists((s) => s.error);
  const p = usePlaylists.getState;
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [drop, setDrop] = useState<{ id: number; zone: DropZone } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);

  const visible = visibleNodes(nodes, tree.expanded);
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

  return (
    <section className="playlists" aria-label="Playlists">
      <div className="sidebar-head">
        <h2>Playlists</h2>
        <button
          type="button"
          title="New playlist"
          onClick={() => void p().create("playlist", selected)}
        >
          +
        </button>
      </div>
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
          className={`tree-row${tree.selectedId === null ? " selected" : ""}`}
          onClick={() => p().select(null)}
        >
          <span className="tree-chevron" />
          Collection
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
      style={{ paddingLeft: 4 + (node.depth + 1) * INDENT }}
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
      <span
        className="tree-chevron"
        onClick={(e) => {
          e.stopPropagation();
          if (node.isFolder) p().dispatch({ type: "toggle", id: node.id });
        }}
      >
        {node.isFolder ? (expanded ? "▾" : "▸") : ""}
      </span>
      {draft === null ? (
        <>
          <span className="tree-name">{node.name}</span>
          {!node.isFolder && <span className="tree-count">{node.entries}</span>}
        </>
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
