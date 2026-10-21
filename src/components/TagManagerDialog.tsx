/**
 * Manage My Tags: add, rename, delete and reorder categories and the tags in
 * them. Names are plain inputs (Enter or leaving the field saves, Escape
 * reverts); order changes with the up/down buttons. reks-core validates
 * names, so a rejected edit shows its reason at the top and changes nothing.
 */
import { useState } from "react";

import { cn } from "../lib/cn";
import type { TagCategory, TagId, TagInfo } from "../lib/ipc";
import { useTags } from "../store/tags";
import { ConfirmDialog } from "./ConfirmDialog";
import { ChevronDownIcon, CloseIcon, PlusIcon } from "./icons";
import { Button } from "./ui/Button";
import { Caption } from "./ui/Caption";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { IconButton } from "./ui/IconButton";
import { Input } from "./ui/Input";

/** What the delete confirmation is about. */
type Doomed = { kind: "category"; item: TagCategory } | { kind: "tag"; item: TagInfo };

/** A name field that saves on Enter / blur when it changed and is not blank. */
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
  const [draft, setDraft] = useState(name);
  function commit() {
    const next = draft.trim();
    if (next === "" || next === name) setDraft(name);
    else onSave(next);
  }
  return (
    <Input
      size="sm"
      className={cn("min-w-0 flex-1", className)}
      aria-label={label}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        else if (e.key === "Escape" && draft !== name) {
          e.stopPropagation();
          setDraft(name);
        }
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
      <Button size="sm" disabled={draft.trim() === ""} onClick={() => void add()}>
        <PlusIcon />
        Add
      </Button>
    </div>
  );
}

/** Up / down / delete for one row; `first` and `last` disable the ends. */
function RowActions({
  what,
  first,
  last,
  onMove,
  onDelete,
}: {
  what: string;
  first: boolean;
  last: boolean;
  onMove: (delta: -1 | 1) => void;
  onDelete: () => void;
}) {
  return (
    <>
      <IconButton
        size="sm"
        aria-label={`Move ${what} up`}
        disabled={first}
        onClick={() => onMove(-1)}
      >
        <span className="inline-flex rotate-180">
          <ChevronDownIcon />
        </span>
      </IconButton>
      <IconButton
        size="sm"
        aria-label={`Move ${what} down`}
        disabled={last}
        onClick={() => onMove(1)}
      >
        <ChevronDownIcon />
      </IconButton>
      <IconButton size="sm" aria-label={`Delete ${what}`} onClick={onDelete}>
        <CloseIcon />
      </IconButton>
    </>
  );
}

function CategorySection({
  category,
  index,
  count,
  onDelete,
}: {
  category: TagCategory;
  index: number;
  count: number;
  onDelete: (d: Doomed) => void;
}) {
  const mutate = useTags((s) => s.mutate);
  return (
    <section className="flex flex-col gap-1.5" aria-label={`Category ${category.name}`}>
      <div className="flex items-center gap-1">
        <Caption className="sr-only">Category</Caption>
        <NameField
          key={`${category.id}:${category.name}`}
          name={category.name}
          label={`Category name: ${category.name}`}
          className="font-semibold"
          onSave={(name) => void mutate({ type: "rename", id: category.id, name })}
        />
        <RowActions
          what={`category ${category.name}`}
          first={index === 0}
          last={index === count - 1}
          onMove={(d) => void mutate({ type: "move", id: category.id, position: index + d })}
          onDelete={() => onDelete({ kind: "category", item: category })}
        />
      </div>
      <ul className="m-0 flex list-none flex-col gap-1 border-l border-border-subtle p-0 pl-3">
        {category.tags.length === 0 && (
          <li className="text-xs text-fg-muted">No tags in this category yet.</li>
        )}
        {category.tags.map((t, i) => (
          <li key={t.id} className="flex items-center gap-1">
            <NameField
              key={`${t.id}:${t.name}`}
              name={t.name}
              label={`Tag name: ${t.name}`}
              onSave={(name) => void mutate({ type: "rename", id: t.id, name })}
            />
            <span className="w-20 shrink-0 text-right text-xs whitespace-nowrap text-fg-subtle tabular-nums">
              {t.trackCount} {t.trackCount === 1 ? "track" : "tracks"}
            </span>
            <RowActions
              what={`tag ${t.name}`}
              first={i === 0}
              last={i === category.tags.length - 1}
              onMove={(d) =>
                void mutate({ type: "move", id: t.id, categoryId: category.id, position: i + d })
              }
              onDelete={() => onDelete({ kind: "tag", item: t })}
            />
          </li>
        ))}
        <li>
          <AddField
            label={`New tag in ${category.name}`}
            placeholder="New tag"
            onAdd={(name) => mutate({ type: "createTag", categoryId: category.id, name })}
          />
        </li>
      </ul>
    </section>
  );
}

function deleteText(d: Doomed): string {
  if (d.kind === "tag") {
    const n = d.item.trackCount;
    return `Delete the tag “${d.item.name}”? ${n === 0 ? "No tracks have it." : `It is removed from ${n} ${n === 1 ? "track" : "tracks"}.`}`;
  }
  const n = d.item.tags.length;
  return `Delete the category “${d.item.name}” and its ${n} ${n === 1 ? "tag" : "tags"}? They are removed from every track.`;
}

export function TagManagerDialog() {
  const open = useTags((s) => s.managerOpen);
  const close = useTags((s) => s.closeManager);
  const tree = useTags((s) => s.tree);
  const status = useTags((s) => s.status);
  const error = useTags((s) => s.error);
  const load = useTags((s) => s.load);
  const mutate = useTags((s) => s.mutate);
  const clearError = useTags((s) => s.clearError);
  const [doomed, setDoomed] = useState<Doomed | null>(null);

  const categories = tree?.categories ?? [];
  const firstCategory: TagId | undefined = categories[0]?.id;
  const noTags = categories.every((c) => c.tags.length === 0);

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(o) => {
          if (!o) {
            clearError();
            close();
          }
        }}
      >
        <DialogContent
          size="md"
          className="h-[560px]"
          onOpenAutoFocus={(e) => {
            // Not into a name field: a stray key would rename a category.
            e.preventDefault();
            (e.currentTarget as HTMLElement).focus();
          }}
        >
          <DialogHeader>
            <DialogTitle>My Tags</DialogTitle>
          </DialogHeader>
          <DialogBody className="flex-1 gap-4">
            {error !== null && (
              <div className="text-danger select-text" role="alert">
                {error}
              </div>
            )}
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
                  <div className="rounded-md border border-border-subtle bg-bg-subtle p-3 text-sm">
                    <p className="m-0 mb-2 text-fg-muted">
                      No tags yet. Tags group by category (Genre, Situation…); add the first one
                      below, then tick it from a track’s right-click menu.
                    </p>
                    <AddField
                      label="Your first tag"
                      placeholder={`First tag, in ${categories[0]?.name}`}
                      onAdd={(name) =>
                        mutate({ type: "createTag", categoryId: firstCategory, name })
                      }
                    />
                  </div>
                )}
                {categories.map((c, i) => (
                  <CategorySection
                    key={c.id}
                    category={c}
                    index={i}
                    count={categories.length}
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
          </DialogBody>
          <DialogFooter>
            <Button variant="primary" onClick={close}>
              Done
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
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
    </>
  );
}
