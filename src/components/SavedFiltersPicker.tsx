/**
 * Saved filters in the Filters drawer (D35, wireframe S1/S2): a button under
 * the title naming the loaded one ("Saved filters" when none) with an
 * "edited" mark, and a popover listing them. A click loads one (Undo toast
 * in the store); each has Rename, Duplicate, Move up / down and Delete
 * (Undo toast) in its menu, and rows drag to reorder. Hovering one shows
 * what its rows say. Data and edits: `filterPresets.ts`, `rules.ts`.
 */
import { type ReactNode, useMemo, useState } from "react";

import { cn } from "../lib/cn";
import type { FilterPreset } from "../lib/ipc";
import { useFilterPresets } from "../store/filterPresets";
import { presetEdited, useRules } from "../store/rules";
import { type CategoryOf, fromRule, summarize } from "../store/ruleTree";
import { useTags } from "../store/tags";
import { useToasts } from "../store/toasts";
import { CheckIcon, ChevronDownIcon, MoreIcon } from "./icons";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";
import { IconButton } from "./ui/IconButton";
import { InlineTextField } from "./ui/InlineText";
import { Input } from "./ui/Input";
import { menuItemVariants } from "./ui/menu";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/Popover";
import { Tooltip } from "./ui/Tooltip";
import { useSummaryNames } from "./useSummaryNames";

/** Lists longer than this get a search field. */
const SEARCH_FROM = 8;

/** Each saved filter's rows in words, for its tooltip. */
function useDescriptions(presets: readonly FilterPreset[]): Map<number, string> {
  const names = useSummaryNames();
  const tree = useTags((s) => s.tree);
  return useMemo(() => {
    const categoryOf: CategoryOf = (tag) =>
      tree?.categories.find((c) => c.tags.some((t) => t.id === tag))?.id;
    return new Map(
      presets.map((p) => [
        p.id,
        p.rule === null
          ? "Saved by a newer reks; can't be read here"
          : (summarize(fromRule(p.rule, categoryOf), names) ?? "Every track"),
      ]),
    );
  }, [presets, names, tree]);
}

export function SavedFiltersPicker() {
  const presets = useFilterPresets((s) => s.presets);
  const status = useFilterPresets((s) => s.status);
  const error = useFilterPresets((s) => s.error);
  const loaded = useRules((s) => s.preset);
  const edited = useRules(presetEdited);
  const descriptions = useDescriptions(presets);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState<number | null>(null);
  const [dragged, setDragged] = useState<number | null>(null);

  const q = query.trim().toLowerCase();
  const shown = q === "" ? presets : presets.filter((p) => p.name.toLowerCase().includes(q));
  const mutate = useFilterPresets.getState().mutate;

  function load(p: FilterPreset) {
    useRules.getState().loadPreset(p);
    setOpen(false);
  }

  /** Moves `id` to where `target` is now. */
  function moveTo(id: number, target: number) {
    const from = presets.findIndex((p) => p.id === id);
    if (from < 0 || from === target) return;
    // `move` counts the position in the list as it is, the moved one included.
    void mutate({ type: "move", id, position: from < target ? target + 1 : target });
  }

  async function remove(p: FilterPreset, index: number) {
    if ((await mutate({ type: "delete", id: p.id })) === null) return;
    const { rule, name } = p;
    useToasts.getState().show(
      `Deleted “${name}”`,
      rule === null
        ? undefined
        : {
            label: "Undo",
            run: () => void mutate({ type: "create", name, rule, position: index }),
          },
    );
  }

  let body: ReactNode;
  if (status === "loading") {
    body = <p className="m-0 px-2 py-1 text-sm text-fg-muted">Loading saved filters…</p>;
  } else if (status === "error") {
    body = (
      <div className="flex flex-col gap-1.5 px-2 py-1 text-sm" role="alert">
        <span className="text-danger select-text">Couldn't load saved filters: {error}</span>
        <span>
          <Button size="sm" onClick={() => void useFilterPresets.getState().load()}>
            Retry
          </Button>
        </span>
      </div>
    );
  } else if (presets.length === 0) {
    body = (
      <p className="m-0 px-2 py-1 text-sm text-fg-muted">
        No saved filters yet. Set some filters, then Save….
      </p>
    );
  } else {
    body = (
      <>
        {presets.length > SEARCH_FROM && (
          <Input
            variant="search"
            size="sm"
            type="search"
            aria-label="Find saved filters"
            placeholder="Search saved filters"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        )}
        <ul
          aria-label="Saved filters"
          className="m-0 flex max-h-72 list-none flex-col overflow-y-auto p-0"
        >
          {shown.map((p) => {
            const index = presets.indexOf(p);
            const isLoaded = loaded?.id === p.id;
            return (
              <li
                key={p.id}
                draggable={renaming === null && q === ""}
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = "move";
                  setDragged(p.id);
                }}
                onDragOver={(e) => {
                  if (dragged !== null) e.preventDefault();
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  if (dragged !== null) moveTo(dragged, index);
                  setDragged(null);
                }}
                onDragEnd={() => setDragged(null)}
                className={cn(
                  "flex items-center gap-0.5 rounded-xs",
                  dragged === p.id && "opacity-45",
                )}
              >
                {renaming === p.id ? (
                  <InlineTextField
                    initial={p.name}
                    aria-label={`Rename ${p.name}`}
                    className="flex-1"
                    onDone={(commit, name) => {
                      setRenaming(null);
                      if (commit && name !== null && name !== p.name)
                        void mutate({ type: "rename", id: p.id, name });
                    }}
                  />
                ) : (
                  <Tooltip content={descriptions.get(p.id)} side="right">
                    <button
                      type="button"
                      aria-current={isLoaded ? "true" : undefined}
                      className={cn(menuItemVariants(), "min-w-0 flex-1 hover:bg-hover")}
                      onClick={() => load(p)}
                    >
                      <span className="flex w-(--ctl-icon-sm) shrink-0 text-accent">
                        {isLoaded && <CheckIcon />}
                      </span>
                      <span className="truncate">{p.name}</span>
                    </button>
                  </Tooltip>
                )}
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <IconButton size="xs" aria-label={`${p.name} options`}>
                      <MoreIcon />
                    </IconButton>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onSelect={() => setRenaming(p.id)}>Rename…</DropdownMenuItem>
                    <DropdownMenuItem onSelect={() => void mutate({ type: "duplicate", id: p.id })}>
                      Duplicate
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      disabled={index === 0}
                      onSelect={() => moveTo(p.id, index - 1)}
                    >
                      Move up
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      disabled={index === presets.length - 1}
                      onSelect={() => moveTo(p.id, index + 1)}
                    >
                      Move down
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem variant="danger" onSelect={() => void remove(p, index)}>
                      Delete
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </li>
            );
          })}
        </ul>
        {shown.length === 0 && (
          <p className="m-0 px-2 py-1 text-sm text-fg-muted">
            No saved filters match “{query.trim()}”.
          </p>
        )}
        {q === "" && presets.length > 1 && (
          <p className="m-0 border-t border-border px-2 pt-1 text-xs text-fg-subtle">
            Drag to reorder
          </p>
        )}
      </>
    );
  }

  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <Popover
        open={open}
        onOpenChange={(o) => {
          if (!o) {
            setQuery("");
            setRenaming(null);
          }
          setOpen(o);
        }}
      >
        <PopoverTrigger asChild>
          <Button
            size="sm"
            variant="ghost"
            className={cn("min-w-0 max-w-48 px-1.5", loaded === null && "text-fg-muted")}
            aria-label={loaded === null ? "Saved filters" : `Saved filter: ${loaded.name}`}
          >
            <span className={cn("truncate", loaded !== null && "font-semibold")}>
              {loaded?.name ?? "Saved filters"}
            </span>
            <ChevronDownIcon />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" className="flex w-64 flex-col gap-1">
          {body}
        </PopoverContent>
      </Popover>
      {edited && (
        <Badge variant="outline" title="Changed since you saved it">
          edited
        </Badge>
      )}
    </span>
  );
}
