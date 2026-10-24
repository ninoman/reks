/**
 * Tag organizer (D36): "Read from Comments…" asks with the counts first; the
 * organizer then shows the run's tags (or all tags, from "Clean up…") by
 * category. Possible duplicates (names that differ only in case, spacing or
 * punctuation, found by reks-core) merge in one click; any ticked tags can be
 * moved to a category (a new one too), merged into one of them or deleted.
 * Names rename on click. State and edits live in `store/tagOrganizer.ts`.
 */
import { useMemo, useState } from "react";

import type { TagCategory, TagId, TagTree } from "../lib/ipc";
import { useTagOrganizer } from "../store/tagOrganizer";
import {
  commentTagsText,
  defaultKeeper,
  groupsInScope,
  scopedCategories,
  type TagRef,
  tagIndex,
} from "../store/tagOrganizerModel";
import { useTags } from "../store/tags";
import { ConfirmDialog } from "./ConfirmDialog";
import { ErrorMessage } from "./ErrorMessage";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Checkbox } from "./ui/Checkbox";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { InlineText } from "./ui/InlineText";
import { Input } from "./ui/Input";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Select } from "./ui/Select";

const sectionTitle = "m-0 text-xs font-semibold text-fg-muted uppercase tracking-wide";

function plural(n: number, word: string): string {
  return `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
}

export function TagOrganizer() {
  const stage = useTagOrganizer((s) => s.stage);
  if (stage === "confirmRead" || stage === "reading") return <ReadQuestion />;
  if (stage === "organize") return <Organizer />;
  return null;
}

function ReadQuestion() {
  const stage = useTagOrganizer((s) => s.stage);
  const preview = useTagOrganizer((s) => s.preview);
  const error = useTagOrganizer((s) => s.error);
  const { confirmRead, close } = useTagOrganizer.getState();
  const nothing = preview !== null && preview.tracks === 0;
  const cancel = { label: nothing ? "Close" : "Cancel", onClick: close };
  return (
    <ConfirmDialog
      title="Read My Tags from Comments"
      buttons={
        nothing || preview === null
          ? [cancel]
          : [{ label: "Read", primary: true, onClick: () => void confirmRead() }, cancel]
      }
    >
      {error !== null && <ErrorMessage as="p" message={error} />}
      <p className="m-0">
        {stage === "reading"
          ? "Reading…"
          : preview === null
            ? "Counting tag blocks…"
            : nothing
              ? "No comment ends with a /* … */ tag block."
              : `Comments with a /* … */ tag block: ${commentTagsText(preview)}. New tags go under "Imported"; comments are not changed.`}
      </p>
    </ConfirmDialog>
  );
}

/** What a confirmation is about. */
type Pending = { kind: "merge"; ids: TagId[] } | { kind: "delete"; ids: TagId[] };

function Organizer() {
  const tree = useTags((s) => s.tree);
  const status = useTags((s) => s.status);
  const tagError = useTags((s) => s.error);
  const runIds = useTagOrganizer((s) => s.runIds);
  const onlyRun = useTagOrganizer((s) => s.onlyRun);
  const duplicates = useTagOrganizer((s) => s.duplicates);
  const error = useTagOrganizer((s) => s.error);
  const org = useTagOrganizer.getState();
  const [selected, setSelected] = useState<ReadonlySet<TagId>>(new Set());
  const [pending, setPending] = useState<Pending | null>(null);

  const scope = useMemo(
    () => (runIds !== null && onlyRun ? new Set(runIds) : null),
    [runIds, onlyRun],
  );
  const index = useMemo(() => (tree === null ? new Map() : tagIndex(tree)), [tree]);
  // Ticks of tags that are gone (merged, deleted) don't count.
  const ticked = [...selected].filter((id) => index.has(id));

  function toggle(id: TagId, on: boolean) {
    const next = new Set(selected);
    if (on) next.add(id);
    else next.delete(id);
    setSelected(next);
  }

  async function run(edit: Promise<void>) {
    await edit;
    setSelected(new Set());
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) org.close();
      }}
    >
      <DialogContent size="lg" className="max-h-[85vh]">
        <DialogHeader>
          <DialogTitle>Organize My Tags</DialogTitle>
          <DialogDescription>
            Group tags into categories, merge duplicates and delete tags you don't need.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          {runIds !== null && (
            <SegmentedControl
              size="sm"
              className="self-start"
              aria-label="Tags shown"
              value={onlyRun ? "run" : "all"}
              onValueChange={(v) => org.setOnlyRun(v === "run")}
            >
              <Segment value="run">Just read ({runIds.length.toLocaleString()})</Segment>
              <Segment value="all">All tags</Segment>
            </SegmentedControl>
          )}
          {tagError !== null && <ErrorMessage as="p" message={tagError} />}
          {error !== null && <ErrorMessage as="p" message={error} />}
          {tree === null ? (
            status === "error" ? (
              <Button
                size="sm"
                className="self-start"
                onClick={() => void useTags.getState().load()}
              >
                Retry
              </Button>
            ) : (
              <p className="m-0 text-sm text-fg-muted">Loading tags…</p>
            )
          ) : (
            <>
              <Duplicates
                groups={duplicates === null ? null : groupsInScope(duplicates, index, scope)}
                onMerge={(ids, into) => void org.merge(ids, into)}
              />
              <TagList
                tree={tree}
                categories={scopedCategories(tree, scope)}
                selected={selected}
                onToggle={toggle}
              />
            </>
          )}
        </DialogBody>
        {tree !== null && (
          <SelectionBar
            tree={tree}
            count={ticked.length}
            onMove={(cat) => void run(org.moveTo(ticked, cat))}
            onMoveToNew={(name) => run(org.moveToNew(ticked, name))}
            onMerge={() => setPending({ kind: "merge", ids: ticked })}
            onDelete={() => setPending({ kind: "delete", ids: ticked })}
            onClear={() => setSelected(new Set())}
          />
        )}
        <DialogFooter>
          <Button variant="primary" onClick={org.close}>
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
      {pending?.kind === "merge" && (
        <MergeQuestion
          refs={pending.ids.flatMap((id) => index.get(id) ?? [])}
          onMerge={(into) => {
            void run(org.merge(pending.ids, into));
            setPending(null);
          }}
          onCancel={() => setPending(null)}
        />
      )}
      {pending?.kind === "delete" && (
        <ConfirmDialog
          title={pending.ids.length === 1 ? "Delete tag" : "Delete tags"}
          buttons={[
            {
              label: "Delete",
              primary: true,
              onClick: () => {
                void run(org.remove(pending.ids));
                setPending(null);
              },
            },
            { label: "Cancel", onClick: () => setPending(null) },
          ]}
        >
          {deleteText(pending.ids.flatMap((id) => index.get(id) ?? []))}
        </ConfirmDialog>
      )}
    </Dialog>
  );
}

function deleteText(refs: TagRef[]): string {
  const tracks = refs.reduce((n, r) => n + r.tag.trackCount, 0);
  const what = refs.length === 1 ? `"${refs[0]?.tag.name}"` : plural(refs.length, "tag");
  return `Delete ${what}? ${tracks === 0 ? "No track has it." : `It comes off ${plural(tracks, "track")}.`}`;
}

function refLabel(r: TagRef): string {
  return `${r.tag.name} (${r.category.name}, ${plural(r.tag.trackCount, "track")})`;
}

function Duplicates({
  groups,
  onMerge,
}: {
  groups: TagRef[][] | null;
  onMerge: (ids: TagId[], into: TagId) => void;
}) {
  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="org-dupes">
      <h3 id="org-dupes" className={sectionTitle}>
        Possible duplicates
      </h3>
      {groups === null ? (
        <p className="m-0 text-sm text-fg-muted">Looking for duplicates…</p>
      ) : groups.length === 0 ? (
        <p className="m-0 text-sm text-fg-muted">No duplicates found.</p>
      ) : (
        groups.map((g) => (
          <DuplicateGroup key={g.map((r) => r.tag.id).join("-")} group={g} onMerge={onMerge} />
        ))
      )}
    </section>
  );
}

function DuplicateGroup({
  group,
  onMerge,
}: {
  group: TagRef[];
  onMerge: (ids: TagId[], into: TagId) => void;
}) {
  const [keep, setKeep] = useState<TagId | null>(defaultKeeper(group));
  const names = group.map((r) => r.tag.name).join(" · ");
  return (
    <div className="flex items-center gap-2 text-sm">
      <span className="min-w-0 flex-1 truncate" title={names}>
        {names}
      </span>
      <Select
        size="sm"
        className="w-72 shrink-0"
        aria-label={`Tag to keep of ${names}`}
        value={keep ?? ""}
        onChange={(e) => setKeep(Number(e.target.value))}
      >
        {group.map((r) => (
          <option key={r.tag.id} value={r.tag.id}>
            Keep {refLabel(r)}
          </option>
        ))}
      </Select>
      <Button
        size="sm"
        disabled={keep === null}
        onClick={() => {
          if (keep !== null)
            onMerge(
              group.map((r) => r.tag.id),
              keep,
            );
        }}
      >
        Merge
      </Button>
    </div>
  );
}

function TagList({
  tree,
  categories,
  selected,
  onToggle,
}: {
  tree: TagTree;
  categories: TagCategory[];
  selected: ReadonlySet<TagId>;
  onToggle: (id: TagId, on: boolean) => void;
}) {
  const mutate = useTags((s) => s.mutate);
  const moveTo = useTagOrganizer((s) => s.moveTo);
  return (
    <section className="flex flex-col gap-3" aria-labelledby="org-tags">
      <h3 id="org-tags" className={sectionTitle}>
        Tags
      </h3>
      {categories.length === 0 && <p className="m-0 text-sm text-fg-muted">No tags to organize.</p>}
      {categories.map((c) => (
        <div key={c.id} className="flex flex-col gap-0.5">
          <h4 className="m-0 text-sm font-semibold">{c.name}</h4>
          {c.tags.length === 0 && <p className="m-0 text-xs text-fg-muted">No tags.</p>}
          {c.tags.map((t) => (
            <div key={t.id} className="flex items-center gap-2">
              <Checkbox
                aria-label={`Select ${t.name}`}
                checked={selected.has(t.id)}
                onChange={(e) => onToggle(t.id, e.target.checked)}
              />
              <InlineText
                editOn="click"
                value={t.name}
                aria-label={`Name of ${t.name}`}
                title="Click to rename"
                className="h-(--ctl-h-sm) min-w-0 flex-1 px-1.5 text-sm"
                onCommit={(next) => {
                  if (next !== null) void mutate({ type: "rename", id: t.id, name: next });
                }}
              />
              <Badge variant="muted" title={plural(t.trackCount, "track")}>
                {t.trackCount.toLocaleString()}
              </Badge>
              <Select
                size="sm"
                className="w-40"
                aria-label={`Category of ${t.name}`}
                value={c.id}
                onChange={(e) => void moveTo([t.id], Number(e.target.value))}
              >
                {tree.categories.map((cat) => (
                  <option key={cat.id} value={cat.id}>
                    {cat.name}
                  </option>
                ))}
              </Select>
            </div>
          ))}
        </div>
      ))}
    </section>
  );
}

function SelectionBar({
  tree,
  count,
  onMove,
  onMoveToNew,
  onMerge,
  onDelete,
  onClear,
}: {
  tree: TagTree;
  count: number;
  onMove: (categoryId: TagId) => void;
  onMoveToNew: (name: string) => Promise<void>;
  onMerge: () => void;
  onDelete: () => void;
  onClear: () => void;
}) {
  const [name, setName] = useState("");
  const none = count === 0;
  async function moveToNew() {
    const n = name.trim();
    if (n === "" || none) return;
    await onMoveToNew(n);
    setName("");
  }
  return (
    <div
      className="flex flex-wrap items-center gap-2 border-t border-border px-5 pt-3 text-sm"
      role="toolbar"
      aria-label="Selected tags"
    >
      <span className="text-fg-muted" aria-live="polite">
        {none ? "Tick tags to move, merge or delete them." : `${plural(count, "tag")} selected`}
      </span>
      <Select
        size="sm"
        className="w-40"
        aria-label="Move selected to category"
        disabled={none}
        value=""
        onChange={(e) => {
          if (e.target.value !== "") onMove(Number(e.target.value));
        }}
      >
        <option value="">Move to…</option>
        {tree.categories.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </Select>
      <Input
        size="sm"
        className="w-36"
        aria-label="New category for selected"
        placeholder="New category"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void moveToNew();
          }
        }}
      />
      <Button size="sm" disabled={none || name.trim() === ""} onClick={() => void moveToNew()}>
        Move to new
      </Button>
      <Button size="sm" disabled={count < 2} onClick={onMerge}>
        Merge…
      </Button>
      <Button size="sm" variant="quiet" disabled={none} onClick={onDelete}>
        Delete…
      </Button>
      {!none && (
        <Button size="sm" variant="link" onClick={onClear}>
          Clear
        </Button>
      )}
    </div>
  );
}

function MergeQuestion({
  refs,
  onMerge,
  onCancel,
}: {
  refs: TagRef[];
  onMerge: (into: TagId) => void;
  onCancel: () => void;
}) {
  const [keep, setKeep] = useState<TagId | null>(defaultKeeper(refs));
  return (
    <ConfirmDialog
      title="Merge tags"
      buttons={[
        {
          label: "Merge",
          primary: true,
          onClick: () => {
            if (keep !== null) onMerge(keep);
          },
        },
        { label: "Cancel", onClick: onCancel },
      ]}
    >
      <p className="mt-0">
        The kept tag goes on every track of the others, then the others are deleted.
      </p>
      <Select
        size="sm"
        aria-label="Tag to keep"
        value={keep ?? ""}
        onChange={(e) => setKeep(Number(e.target.value))}
      >
        {refs.map((r) => (
          <option key={r.tag.id} value={r.tag.id}>
            Keep {refLabel(r)}
          </option>
        ))}
      </Select>
    </ConfirmDialog>
  );
}
