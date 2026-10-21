/**
 * "My Tags" submenu of the track context menu: a checkbox per tag, grouped by
 * category, for the selected tracks. A tag only some of them have shows as
 * mixed; clicking it adds it to all of them (`clickAction`). The menu stays
 * open while several tags are ticked. The search field filters the list and
 * offers "Create 'X'" per category.
 */
import { type KeyboardEvent, useState } from "react";

import type { TagId } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { clickAction, coverage, creatableIn, searchTags } from "../store/tagModel";
import { useTags } from "../store/tags";
import {
  ContextMenuCheckboxItem,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "./ui/ContextMenu";
import { Input } from "./ui/Input";

/** Tags of each of `trackIds` that is on a loaded page. */
function useSelectedTags(trackIds: readonly number[]): TagId[][] {
  const pages = useLibrary((s) => s.pages);
  const wanted = new Set(trackIds);
  const found = new Map<number, TagId[]>();
  for (const rows of pages.values())
    for (const r of rows) if (wanted.has(r.id)) found.set(r.id, r.tags);
  return [...wanted].map((id) => found.get(id) ?? []);
}

/** Keeps typing in the field from reaching the menu's typeahead and arrow handling. */
function fieldKeys(e: KeyboardEvent<HTMLInputElement>) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    e.currentTarget
      .closest<HTMLElement>("[data-radix-menu-content]")
      ?.querySelector<HTMLElement>('[role="menuitemcheckbox"], [role="menuitem"]')
      ?.focus();
    return;
  }
  if (e.key !== "Escape" && e.key !== "Tab") e.stopPropagation();
}

export function TagSubmenu({ trackIds }: { trackIds: readonly number[] }) {
  const tree = useTags((s) => s.tree);
  const status = useTags((s) => s.status);
  const error = useTags((s) => s.error);
  const openManager = useTags((s) => s.openManager);
  const [query, setQuery] = useState("");
  const selected = useSelectedTags(trackIds);

  const tags = useTags.getState();
  const shown = tree === null ? [] : searchTags(tree, query);
  const create = tree === null ? [] : creatableIn(tree, query);
  const name = query.trim();
  const noTags = tree?.categories.every((c) => c.tags.length === 0) === true;

  return (
    <ContextMenuSub
      onOpenChange={(open) => {
        if (!open) setQuery("");
      }}
    >
      <ContextMenuSubTrigger disabled={trackIds.length === 0}>My Tags</ContextMenuSubTrigger>
      <ContextMenuSubContent className="max-h-80 w-56 overflow-auto">
        {tree === null ? (
          <ContextMenuItem disabled>
            {status === "error" ? (error ?? "Could not load tags") : "Loading tags…"}
          </ContextMenuItem>
        ) : (
          <>
            <div className="p-1">
              <Input
                variant="search"
                size="sm"
                className="w-full"
                placeholder="Find or create a tag"
                aria-label="Find a tag"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={fieldKeys}
              />
            </div>
            {noTags && name === "" && (
              <ContextMenuItem disabled>No tags yet. Type a name to create one.</ContextMenuItem>
            )}
            {shown.map((c) => (
              <ContextMenuGroup key={c.id}>
                <ContextMenuLabel>{c.name}</ContextMenuLabel>
                {c.tags.map((t) => {
                  const cov = coverage(selected, t.id);
                  return (
                    <ContextMenuCheckboxItem
                      key={t.id}
                      checked={cov === "all" ? true : cov === "some" ? "indeterminate" : false}
                      // Keep the menu open to tick several tags in a row.
                      onSelect={(e) => {
                        e.preventDefault();
                        void (clickAction(cov) === "assign"
                          ? tags.assign([...trackIds], [t.id])
                          : tags.unassign([...trackIds], [t.id]));
                      }}
                    >
                      {t.name}
                    </ContextMenuCheckboxItem>
                  );
                })}
              </ContextMenuGroup>
            ))}
            {create.length > 0 && <ContextMenuSeparator />}
            {create.map((c) => (
              <ContextMenuItem
                key={c.id}
                onSelect={() => void tags.createAndAssign(c.id, name, [...trackIds])}
              >
                Create “{name}” in {c.name}
              </ContextMenuItem>
            ))}
            {shown.length === 0 && create.length === 0 && name !== "" && (
              <ContextMenuItem disabled>No tag matches “{name}”</ContextMenuItem>
            )}
          </>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={openManager}>Manage tags…</ContextMenuItem>
      </ContextMenuSubContent>
    </ContextMenuSub>
  );
}
