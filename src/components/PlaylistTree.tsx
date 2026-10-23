/**
 * The playlist tree in the sidebar. Click selects (and shows it in the
 * table), Shift+click opens it in a split pane under the table (its row is
 * marked too), F2 / Enter renames in place, Delete asks first. Right-click for the
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
import { useHistory } from "../store/history";
import { usePanes } from "../store/panes";
import { usePlaylists } from "../store/playlists";
import { visibleNodes } from "../store/playlistTree";
import { useRules } from "../store/rules";
import { Disclosure } from "./Disclosure";
import { ErrorMessage } from "./ErrorMessage";
import { FolderIcon, InfoIcon, PlaylistIcon, PlusIcon } from "./icons";
import { DISCLOSURE, ICON, INDENT, NAME, PAD, ROW } from "./treeStyles";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "./ui/ContextMenu";
import { focusRingInset } from "./ui/control";
import { IconButton } from "./ui/IconButton";
import { Input } from "./ui/Input";
import { ScrollArea } from "./ui/ScrollArea";
import { Tooltip } from "./ui/Tooltip";

export function PlaylistTree() {
  const nodes = usePlaylists((s) => s.nodes);
  const tree = usePlaylists((s) => s.tree);
  const error = usePlaylists((s) => s.error);
  /** A history session in the main area: no playlist row is selected. */
  const sessionOpen = useHistory((s) => s.openId !== null);
  const panes = usePanes((s) => s.panes);
  const p = usePlaylists.getState;
  /** The node the context menu is for (`null`: the tree itself). */
  const [menuNode, setMenuNode] = useState<PlaylistNode | null>(null);
  /** Set by a row's right-click, read by the tree's (which opens the menu). */
  const hit = useRef<PlaylistNode | null>(null);
  const [drop, setDrop] = useState<{ id: number; zone: DropZone } | null>(null);
  /** The smart playlist tracks are being dragged over (they can't be dropped there). */
  const [refused, setRefused] = useState<number | null>(null);

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
      if (node.smart) {
        setRefused(node.id);
        return;
      }
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
    setRefused(null);
    const tracks = readTracks(e.dataTransfer);
    if (tracks !== null) {
      if (!node.isFolder && !node.smart) void p().addTracks(node.id, tracks.trackIds);
      return;
    }
    const moved = readNode(e.dataTransfer);
    if (moved !== null) void p().moveNode(moved, node, zone);
  }

  const rootCount = nodes.filter((n) => n.parentId === null).length;

  return (
    <section className="flex min-h-0 flex-1 flex-col" aria-label="Playlists">
      {error !== null && <ErrorMessage className="mx-2 my-1.5" message={error} />}
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
                aria-selected={tree.selectedId === null && !sessionOpen}
                className={ROW}
                data-root=""
                data-selected={(tree.selectedId === null && !sessionOpen) || undefined}
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
                <Tooltip content={SPLIT_HINT} side="right" className="block max-w-64">
                  <IconButton
                    size="xs"
                    shape="round"
                    aria-label="About opening several playlists"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <InfoIcon />
                  </IconButton>
                </Tooltip>
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
                  selected={tree.selectedId === node.id && !sessionOpen}
                  inPane={!sessionOpen && panes.some((pane) => pane.playlistId === node.id)}
                  expanded={tree.expanded.has(node.id)}
                  draft={tree.renaming?.id === node.id ? tree.renaming.draft : null}
                  dropZone={drop?.id === node.id ? drop.zone : null}
                  onContextMenu={() => {
                    hit.current = node;
                  }}
                  onDragOver={(e) => onDragOver(e, node)}
                  refused={refused === node.id}
                  onDragLeave={() => {
                    setDrop((d) => (d?.id === node.id ? null : d));
                    setRefused((r) => (r === node.id ? null : r));
                  }}
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
              <ContextMenuSeparator />
              <ContextMenuItem onSelect={() => excludeFromView(menuNode)}>
                {menuNode.isFolder
                  ? "Show tracks not in this folder"
                  : "Show tracks not in this playlist"}
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() =>
                  p().dispatch({ type: "startRename", id: menuNode.id, name: menuNode.name })
                }
              >
                Rename
              </ContextMenuItem>
              {menuNode.smart && (
                <ContextMenuItem onSelect={() => void useRules.getState().editSmart(menuNode)}>
                  Edit rules
                </ContextMenuItem>
              )}
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

/**
 * Quick path: shows the whole collection without the tracks of `node` (a row
 * "not in playlist" in the rule builder). The tree selects a right-clicked
 * node, so the collection is shown first or the result would be empty.
 */
function excludeFromView(node: PlaylistNode) {
  usePlaylists.getState().select(null);
  useRules.getState().addCondition({ field: "playlist", id: node.id }, true);
}

/** The Playlists row's (i) tooltip: how to open several lists at once. */
const SPLIT_HINT = (
  <>
    <p className="m-0">Shift+click a playlist to open it under the track table.</p>
    <p className="m-0 mt-1">
      Each open list has its own sort and selection and an × to close it. Search, filters and the
      info panel apply to the top list. Drag tracks between lists to add them.
    </p>
  </>
);

/** Drop-target id of the "Playlists" root row (ids from SQLite are positive). */
const ROOT = -1;

function CollectionIcon() {
  return (
    <svg className={ICON} viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="8" cy="8" r="1.8" fill="currentColor" />
    </svg>
  );
}

/** A playlist whose tracks follow rules: the list icon with a spark. */
function SmartPlaylistIcon() {
  return (
    <svg className={ICON} viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <path
        d="M10.5 1.5H3a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
      />
      <path d="M4.5 5h4M4.5 8h3M4.5 11h3" stroke="currentColor" strokeWidth="1.3" />
      <path d="M12.5 4.5l.9 1.9 1.9.9-1.9.9-.9 1.9-.9-1.9-1.9-.9 1.9-.9z" fill="currentColor" />
    </svg>
  );
}

function TreeRow({
  node,
  selected,
  inPane,
  expanded,
  draft,
  dropZone,
  refused,
  onContextMenu,
  onDragOver,
  onDragLeave,
  onDrop,
}: {
  node: PlaylistNode;
  selected: boolean;
  /** Shown in a split pane: marked like the selection, but not the tree's cursor. */
  inPane: boolean;
  expanded: boolean;
  draft: string | null;
  dropZone: DropZone | null;
  refused: boolean;
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
      data-selected={selected || inPane || undefined}
      data-drop={dropZone ?? undefined}
      style={{ paddingLeft: PAD + (node.depth + 1) * INDENT }}
      title={
        node.isFolder ? undefined : `${node.entries} tracks${node.smart ? " · smart playlist" : ""}`
      }
      draggable={draft === null}
      onClick={(e) => (e.shiftKey ? p().openInPane(node.id) : p().select(node.id))}
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
      {node.isFolder ? (
        <FolderIcon className={ICON} />
      ) : node.smart ? (
        <SmartPlaylistIcon />
      ) : (
        <PlaylistIcon className={ICON} />
      )}
      {draft === null ? (
        <>
          <span className={NAME}>{node.name}</span>
          {refused && (
            <span className="text-xs text-fg-muted">Follows its rules: can't add tracks</span>
          )}
        </>
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
