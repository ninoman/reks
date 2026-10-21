/**
 * The My Tags picker body (inside a Popover): a search field, checkboxes
 * grouped by category, and "Create 'X'" for a name that does not exist yet.
 * The caller says what each box shows (`coverageOf`) and what a click does.
 */
import { useState } from "react";

import type { TagId } from "../lib/ipc";
import { creatableIn, searchTags, type TagCoverage } from "../store/tagModel";
import { useTags } from "../store/tags";
import { Button } from "./ui/Button";
import { Caption } from "./ui/Caption";
import { Checkbox } from "./ui/Checkbox";
import { Input } from "./ui/Input";

export interface TagPickerProps {
  coverageOf: (tag: TagId) => TagCoverage;
  onToggle: (tag: TagId, coverage: TagCoverage) => void;
  /** Offers "Create 'X'" per category when set. */
  onCreate?: (categoryId: TagId, name: string) => void;
}

export function TagPicker({ coverageOf, onToggle, onCreate }: TagPickerProps) {
  const tree = useTags((s) => s.tree);
  const status = useTags((s) => s.status);
  const error = useTags((s) => s.error);
  const load = useTags((s) => s.load);
  const openManager = useTags((s) => s.openManager);
  const [query, setQuery] = useState("");

  if (tree === null) {
    return status === "error" ? (
      <div className="flex w-64 flex-col gap-1.5 p-1.5 text-sm" role="alert">
        <span className="text-danger select-text">{error}</span>
        <Button size="sm" onClick={() => void load()}>
          Retry
        </Button>
      </div>
    ) : (
      <p className="m-0 w-64 p-1.5 text-sm text-fg-muted">Loading tags…</p>
    );
  }

  if (tree.categories.every((c) => c.tags.length === 0) && onCreate === undefined) {
    return (
      <div className="flex w-64 flex-col items-start gap-1.5 p-1.5 text-sm">
        <p className="m-0 text-fg-muted">No tags yet.</p>
        <Button size="sm" onClick={openManager}>
          Create your first tag
        </Button>
      </div>
    );
  }

  const shown = searchTags(tree, query);
  const create = onCreate === undefined ? [] : creatableIn(tree, query);
  const name = query.trim();

  return (
    <div className="flex w-64 flex-col gap-1.5">
      <Input
        variant="search"
        size="sm"
        className="w-full"
        placeholder="Find or create a tag"
        aria-label="Find a tag"
        value={query}
        autoFocus
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          const first = create[0];
          if (e.key === "Enter" && shown.length === 0 && first !== undefined && onCreate) {
            e.preventDefault();
            onCreate(first.id, name);
            setQuery("");
          }
        }}
      />
      <div className="flex max-h-72 flex-col gap-1.5 overflow-auto">
        {shown.length === 0 && name === "" && (
          <p className="m-0 px-1 text-sm text-fg-muted">No tags yet. Type a name to create one.</p>
        )}
        {shown.length === 0 && name !== "" && create.length === 0 && (
          <p className="m-0 px-1 text-sm text-fg-muted">No tag matches “{name}”.</p>
        )}
        {shown.map((c) => (
          <fieldset key={c.id} className="m-0 flex flex-col border-0 p-0">
            <Caption asChild>
              <legend className="px-1">{c.name}</legend>
            </Caption>
            {c.tags.map((t) => {
              const cov = coverageOf(t.id);
              return (
                <label
                  key={t.id}
                  className="flex h-(--menu-item-h) cursor-default items-center gap-2 rounded-xs px-1 text-sm hover:bg-hover"
                >
                  <Checkbox
                    checked={cov === "all"}
                    indeterminate={cov === "some"}
                    onChange={() => onToggle(t.id, cov)}
                  />
                  <span className="min-w-0 flex-1 truncate">{t.name}</span>
                  <span className="text-xs text-fg-subtle tabular-nums">{t.trackCount}</span>
                </label>
              );
            })}
          </fieldset>
        ))}
        {create.map((c) => (
          <Button
            key={c.id}
            size="sm"
            variant="ghost"
            className="justify-start font-normal"
            onClick={() => {
              onCreate?.(c.id, name);
              setQuery("");
            }}
          >
            Create “{name}” in {c.name}
          </Button>
        ))}
        {tree.categories.length === 0 && (
          <Button size="sm" onClick={openManager}>
            Create a category first
          </Button>
        )}
      </div>
    </div>
  );
}
