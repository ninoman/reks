/**
 * Drag-and-drop reordering in the My Tags drawer: a grip starts the drag,
 * rows mark where the drop lands, and the drop becomes a `move` through the
 * tags store (`dropMove` decides what, if anything, moves). Tags and
 * categories use their own MIME types so each row only answers its kind.
 */
import { type DragEvent, useRef, useState } from "react";

import { hasType, TAG_CATEGORY_MIME, TAG_MIME } from "../lib/dnd";
import { dropMove, type TagDropZone, type TagNodeRef } from "../store/tagModel";
import { useTags } from "../store/tags";

const mime = (kind: TagNodeRef["kind"]) => (kind === "tag" ? TAG_MIME : TAG_CATEGORY_MIME);

/** Upper or lower half of the row under the pointer. */
function half(e: DragEvent<HTMLElement>): TagDropZone {
  const rect = e.currentTarget.getBoundingClientRect();
  return e.clientY - rect.top < rect.height / 2 ? "before" : "after";
}

export function useTagDnd() {
  const dragged = useRef<TagNodeRef | null>(null);
  /** The row showing a drop mark; a category is a target twice (section for categories, header for tags). */
  const [drop, setDrop] = useState<{ id: number; accepts: string; zone: TagDropZone } | null>(null);

  return {
    /** Props for the grip of `node`; the drag image is the enclosing `[data-drag-row]`. */
    source(node: TagNodeRef) {
      return {
        draggable: true,
        onDragStart(e: DragEvent<HTMLElement>) {
          dragged.current = node;
          e.dataTransfer.setData(mime(node.kind), String(node.id));
          e.dataTransfer.effectAllowed = "move";
          const row = e.currentTarget.closest<HTMLElement>("[data-drag-row]");
          if (row !== null) e.dataTransfer.setDragImage(row, 8, row.offsetHeight / 2);
        },
        onDragEnd() {
          dragged.current = null;
          setDrop(null);
        },
      };
    },
    /**
     * Props for a row taking drops of `accepts`: "into" for a category header
     * taking tags, otherwise above / below by pointer position.
     */
    target(node: TagNodeRef, accepts: TagNodeRef["kind"]) {
      const zoneOf = (e: DragEvent<HTMLElement>): TagDropZone =>
        node.kind === "category" && accepts === "tag" ? "into" : half(e);
      const here = (d: typeof drop) => d?.id === node.id && d.accepts === accepts;
      return {
        "data-drop": here(drop) ? drop?.zone : undefined,
        onDragOver(e: DragEvent<HTMLElement>) {
          if (!hasType(e.dataTransfer, mime(accepts))) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          const zone = zoneOf(e);
          if (!here(drop) || drop?.zone !== zone) setDrop({ id: node.id, accepts, zone });
        },
        onDragLeave(e: DragEvent<HTMLElement>) {
          // Moving onto a child fires leave on the row; only a real exit clears it.
          if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
          setDrop((d) => (here(d) ? null : d));
        },
        onDrop(e: DragEvent<HTMLElement>) {
          if (!hasType(e.dataTransfer, mime(accepts))) return;
          e.preventDefault();
          e.stopPropagation();
          setDrop(null);
          const from = dragged.current;
          const { tree, mutate } = useTags.getState();
          if (from === null || tree === null) return;
          const op = dropMove(tree, from, node, zoneOf(e));
          if (op !== null) void mutate(op);
        },
      };
    },
  };
}

export type TagDnd = ReturnType<typeof useTagDnd>;
