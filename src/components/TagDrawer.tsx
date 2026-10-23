/**
 * My Tags drawer (right of the table, over the info panel): attach tags to
 * the selected tracks and add, rename, delete and reorder categories and tags
 * in the same place. A tag's checkbox shows whether the selection has it
 * (mixed when only some tracks do; no checkboxes with nothing selected); a tag added while tracks are selected is
 * attached to them. Rows reorder by dragging their grip (a tag can also go
 * to another category); × deletes after a confirmation. Names read as text and turn
 * into a field on click (Enter or leaving it saves, Escape reverts);
 * reks-core validates them, so a rejected edit shows its reason at the top
 * and changes nothing. Escape elsewhere closes the drawer.
 */
import { useState } from "react";

import { cn } from "../lib/cn";
import type { TagCategory, TagId, TagInfo, TrackRow } from "../lib/ipc";
import { clickAction, coverage, type TagCoverage } from "../store/tagModel";
import { useTags } from "../store/tags";
import { useTagTargets } from "../store/tagTargets";
import { ConfirmDialog } from "./ConfirmDialog";
import { ErrorMessage } from "./ErrorMessage";
import { CloseIcon, GripIcon, PlusIcon } from "./icons";
import { Button } from "./ui/Button";
import { Checkbox } from "./ui/Checkbox";
import { IconButton } from "./ui/IconButton";
import { InlineText } from "./ui/InlineText";
import { Input } from "./ui/Input";
import { ScrollArea } from "./ui/ScrollArea";
import { SheetHeader } from "./ui/Sheet";
import { type TagDnd, useTagDnd } from "./useTagDnd";

/** The line a drag would drop at: above, below, or around (into) the row. */
const DROP_MARKS = [
  "rounded-xs",
  "data-[drop=into]:shadow-[0_0_0_2px_var(--color-accent)]",
  "data-[drop=before]:shadow-[0_-2px_0_var(--color-accent)]",
  "data-[drop=after]:shadow-[0_2px_0_var(--color-accent)]",
].join(" ");

/** The drag handle; mouse only, the "…" menu moves rows from the keyboard. */
function Grip({ label, dnd }: { label: string; dnd: ReturnType<TagDnd["source"]> }) {
  return (
    <span
      className="flex shrink-0 cursor-grab text-fg-subtle hover:text-fg active:cursor-grabbing [&_svg]:size-(--ctl-icon-sm)"
      title={label}
      aria-hidden
      {...dnd}
    >
      <GripIcon />
    </span>
  );
}

/** What the delete confirmation is about. */
type Doomed = { kind: "category"; item: TagCategory } | { kind: "tag"; item: TagInfo };

/** A name shown as text; a click turns it into a field (Enter or leaving saves, Escape reverts). */
function NameField({
  name,
  label,
  className,
  onSave,
}: {
  name: string;
  label: string;
  className?: string;
  onSave: (name: string) => void;
}) {
  return (
    <InlineText
      editOn="click"
      value={name}
      aria-label={label}
      title="Click to rename"
      className={cn("h-(--ctl-h-sm) min-w-0 flex-1 px-1.5 text-sm", className)}
      onCommit={(next) => {
        if (next !== null) onSave(next);
      }}
    />
  );
}

/** A name to create something with: Enter or the button adds it. */
function AddField({
  label,
  placeholder,
  onAdd,
}: {
  label: string;
  placeholder: string;
  /** Resolves `null` when the edit was rejected: the text stays to be fixed. */
  onAdd: (name: string) => Promise<unknown>;
}) {
  const [draft, setDraft] = useState("");
  async function add() {
    const name = draft.trim();
    if (name === "") return;
    if ((await onAdd(name)) !== null) setDraft("");
  }
  return (
    <div className="flex items-center gap-1.5">
      <Input
        size="sm"
        className="min-w-0 flex-1"
        aria-label={label}
        placeholder={placeholder}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void add();
          }
        }}
      />
      <IconButton
        size="sm"
        aria-label={label}
        disabled={draft.trim() === ""}
        onClick={() => void add()}
      >
        <PlusIcon />
      </IconButton>
    </div>
  );
}

/** Deletes one row (after a confirmation). */
function DeleteButton({ what, onDelete }: { what: string; onDelete: () => void }) {
  return (
    <IconButton size="sm" aria-label={`Delete ${what}`} onClick={onDelete}>
      <CloseIcon />
    </IconButton>
  );
}

function TagRow({
  tag,
  targets,
  dnd,
  onDelete,
}: {
  tag: TagInfo;
  targets: TrackRow[];
  dnd: TagDnd;
  onDelete: (d: Doomed) => void;
}) {
  const node = { kind: "tag", id: tag.id } as const;
  const { mutate, assign, unassign } = useTags.getState();
  const cov: TagCoverage = coverage(
    targets.map((r) => r.tags),
    tag.id,
  );
  const ids = targets.map((r) => r.id);
  return (
    <li
      className={cn("flex items-center gap-1.5", DROP_MARKS)}
      data-drag-row
      {...dnd.target(node, "tag")}
    >
      <Grip label={`Drag to move ${tag.name}`} dnd={dnd.source(node)} />
      {ids.length > 0 && (
        <Checkbox
          aria-label={`Attach ${tag.name}`}
          checked={cov === "all"}
          indeterminate={cov === "some"}
          onChange={() =>
            void (clickAction(cov) === "assign" ? assign(ids, [tag.id]) : unassign(ids, [tag.id]))
          }
        />
      )}
      <NameField
        key={`${tag.id}:${tag.name}`}
        name={tag.name}
        label={`Tag name: ${tag.name}`}
        onSave={(name) => void mutate({ type: "rename", id: tag.id, name })}
      />
      <span
        className="w-8 shrink-0 text-right text-xs text-fg-subtle tabular-nums"
        title={`${tag.trackCount} ${tag.trackCount === 1 ? "track" : "tracks"}`}
      >
        {tag.trackCount}
      </span>
      <DeleteButton
        what={`tag ${tag.name}`}
        onDelete={() => onDelete({ kind: "tag", item: tag })}
      />
    </li>
  );
}

function CategorySection({
  category,
  targets,
  dnd,
  onDelete,
}: {
  category: TagCategory;
  targets: TrackRow[];
  dnd: TagDnd;
  onDelete: (d: Doomed) => void;
}) {
  const mutate = useTags((s) => s.mutate);
  const node = { kind: "category", id: category.id } as const;
  return (
    <section
      className={cn("flex flex-col gap-1", DROP_MARKS)}
      aria-label={`Category ${category.name}`}
      data-drag-row
      {...dnd.target(node, "category")}
    >
      <div className={cn("flex items-center gap-1.5", DROP_MARKS)} {...dnd.target(node, "tag")}>
        <Grip label={`Drag to move ${category.name}`} dnd={dnd.source(node)} />
        <NameField
          key={`${category.id}:${category.name}`}
          name={category.name}
          label={`Category name: ${category.name}`}
          className="font-semibold"
          onSave={(name) => void mutate({ type: "rename", id: category.id, name })}
        />
        <DeleteButton
          what={`category ${category.name}`}
          onDelete={() => onDelete({ kind: "category", item: category })}
        />
      </div>
      <ul className="m-0 flex list-none flex-col gap-1 p-0 pl-1">
        {category.tags.length === 0 && (
          <li className="px-1 text-xs text-fg-muted">No tags in this category yet.</li>
        )}
        {category.tags.map((t) => (
          <TagRow key={t.id} tag={t} targets={targets} dnd={dnd} onDelete={onDelete} />
        ))}
        <li>
          <AddField
            label={`New tag in ${category.name}`}
            placeholder={`New tag in ${category.name}`}
            onAdd={(name) => addTag(category.id, name, targets)}
          />
        </li>
      </ul>
    </section>
  );
}

/** Creates a tag and attaches it to `targets`; `null` when the name was rejected. */
async function addTag(categoryId: TagId, name: string, targets: TrackRow[]) {
  const { mutate, assign } = useTags.getState();
  const outcome = await mutate({ type: "createTag", categoryId, name });
  if (outcome?.createdId != null && targets.length > 0)
    await assign(
      targets.map((r) => r.id),
      [outcome.createdId],
    );
  return outcome;
}

function deleteText(d: Doomed): string {
  if (d.kind === "tag") {
    const n = d.item.trackCount;
    return `Delete the tag “${d.item.name}”? ${n === 0 ? "No tracks have it." : `It is removed from ${n} ${n === 1 ? "track" : "tracks"}.`}`;
  }
  const n = d.item.tags.length;
  return `Delete the category “${d.item.name}” and its ${n} ${n === 1 ? "tag" : "tags"}? They are removed from every track.`;
}

function targetText(targets: TrackRow[]): string {
  if (targets.length === 0) return "Select tracks to attach tags to them.";
  if (targets.length > 1) return `Tick a tag to attach it to ${targets.length} selected tracks.`;
  const r = targets[0];
  const name = [r?.artist, r?.title].filter(Boolean).join(" – ") || "Untitled";
  return `Tick a tag to attach it to ${name}.`;
}

export function TagDrawer() {
  const close = useTags((s) => s.closeManager);
  const tree = useTags((s) => s.tree);
  const status = useTags((s) => s.status);
  const error = useTags((s) => s.error);
  const load = useTags((s) => s.load);
  const mutate = useTags((s) => s.mutate);
  const targets = useTagTargets();
  const dnd = useTagDnd();
  const [doomed, setDoomed] = useState<Doomed | null>(null);

  const categories = tree?.categories ?? [];
  const firstCategory: TagId | undefined = categories[0]?.id;
  const noTags = categories.every((c) => c.tags.length === 0);

  return (
    <aside
      className="flex min-h-0 flex-1 flex-col"
      aria-label="My Tags"
      onKeyDown={(e) => {
        // Not from a portalled menu or the confirmation: those close themselves.
        if (e.key === "Escape" && e.currentTarget.contains(e.target as Node)) {
          e.stopPropagation();
          close();
        }
      }}
    >
      <div className="px-3 pt-2">
        <SheetHeader title="My Tags" closeLabel="Close My Tags" onClose={close} />
        <p className="m-0 pb-2 text-xs text-fg-muted" aria-live="polite">
          {targetText(targets)}
        </p>
      </div>
      {error !== null && <ErrorMessage className="px-3 pb-2 text-sm" message={error} />}
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-4 px-3 pt-1 pb-3">
          {tree === null ? (
            status === "error" ? (
              <Button size="sm" className="self-start" onClick={() => void load()}>
                Retry
              </Button>
            ) : (
              <p className="m-0 text-sm text-fg-muted">Loading tags…</p>
            )
          ) : (
            <>
              {noTags && firstCategory !== undefined && (
                <div className="rounded-md border border-border-subtle bg-bg p-3 text-sm">
                  <p className="m-0 mb-2 text-fg-muted">
                    No tags yet. Tags group by category (Genre, Situation…); add the first one
                    below.
                  </p>
                  <AddField
                    label="Your first tag"
                    placeholder={`First tag, in ${categories[0]?.name}`}
                    onAdd={(name) => addTag(firstCategory, name, targets)}
                  />
                </div>
              )}
              {categories.map((c) => (
                <CategorySection
                  key={c.id}
                  category={c}
                  targets={targets}
                  dnd={dnd}
                  onDelete={setDoomed}
                />
              ))}
              <AddField
                label="New category"
                placeholder="New category"
                onAdd={(name) => mutate({ type: "createCategory", name })}
              />
            </>
          )}
        </div>
      </ScrollArea>
      {doomed !== null && (
        <ConfirmDialog
          title={doomed.kind === "tag" ? "Delete tag" : "Delete category"}
          buttons={[
            {
              label: "Delete",
              primary: true,
              onClick: () => {
                void mutate({ type: "delete", id: doomed.item.id });
                setDoomed(null);
              },
            },
            { label: "Cancel", onClick: () => setDoomed(null) },
          ]}
        >
          {deleteText(doomed)}
        </ConfirmDialog>
      )}
    </aside>
  );
}
