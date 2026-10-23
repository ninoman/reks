/**
 * Filters for the track list (D35): a drawer on the right, like My Tags,
 * shown and hidden from the toolbar's Filters button. One query: the default
 * fields as rows from the start, more from "+ Add filter", ANDed until a
 * connector or Or… says "or" (`RuleBuilder`). It narrows whatever the tree
 * shows. The on/off switch lives in the toolbar next to that button, so it
 * works with the drawer hidden. Escape closes the drawer; `/` (outside a text
 * field) opens Add filter.
 */
import { type ReactNode, useState } from "react";

import { cn } from "../lib/cn";
import { useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { clearAllFilters, useRules } from "../store/rules";
import { activeRowCount, isBlank, toRule } from "../store/ruleTree";
import { RuleBuilder } from "./RuleBuilder";
import { SaveSmartDialog } from "./SaveSmartDialog";
import { Button } from "./ui/Button";
import { ScrollArea } from "./ui/ScrollArea";
import { SheetHeader } from "./ui/Sheet";
import { Switch } from "./ui/Switch";
import { Tooltip } from "./ui/Tooltip";

/** The on/off switch: applies the filters or sets them aside (values kept). */
export function FiltersSwitch() {
  const enabled = useLibrary((s) => s.filtersEnabled);
  const setEnabled = useLibrary((s) => s.setFiltersEnabled);
  return (
    <Tooltip
      content={enabled ? "Filters on: click to set them aside" : "Filters off: click to apply"}
    >
      <Switch aria-label="Apply filters" checked={enabled} onCheckedChange={setEnabled} />
    </Tooltip>
  );
}

/** Typing goes to the field, not to the drawer's shortcuts. */
function isTextEntry(t: EventTarget | null): boolean {
  return (
    t instanceof HTMLInputElement ||
    t instanceof HTMLTextAreaElement ||
    (t instanceof HTMLElement && t.isContentEditable)
  );
}

/** A note above the rows with its actions. */
function Banner({ children }: { children: ReactNode }) {
  return (
    <div className="mx-3 mt-1 flex flex-col gap-1.5 rounded-md border border-border bg-surface px-2 py-1.5 text-sm">
      {children}
    </div>
  );
}

export function FilterDrawer() {
  const root = useRules((s) => s.root);
  const editing = useRules((s) => s.editing);
  const enabled = useLibrary((s) => s.filtersEnabled);
  const setEnabled = useLibrary((s) => s.setFiltersEnabled);
  const close = usePanels((s) => s.toggleFilterBar);
  const [saveOpen, setSaveOpen] = useState(false);

  const finished = toRule(root) !== null;
  const kept = activeRowCount(root);

  return (
    <aside
      className="flex min-h-0 flex-1 flex-col"
      aria-label="Filters"
      onKeyDown={(e) => {
        // Not from a portalled picker: those close themselves.
        if (e.key === "Escape" && e.currentTarget.contains(e.target as Node)) {
          e.stopPropagation();
          close();
        } else if (
          e.key === "/" &&
          !e.metaKey &&
          !e.ctrlKey &&
          !e.altKey &&
          !isTextEntry(e.target)
        ) {
          e.preventDefault();
          useRules.getState().setAddOpen(true);
        }
      }}
    >
      <div className="px-3 pt-2">
        <SheetHeader title="Filters" closeLabel="Close Filters" onClose={close} />
      </div>
      <div className="flex items-center justify-end px-3">
        <Button size="sm" variant="link" disabled={isBlank(root)} onClick={clearAllFilters}>
          Clear all
        </Button>
      </div>

      {editing !== null && (
        <Banner>
          <span>
            Editing <strong className="font-semibold">{editing.name}</strong> (smart playlist). The
            table previews your whole library.
          </span>
          <span className="flex gap-1.5">
            <Button
              size="sm"
              variant="primary"
              disabled={!finished}
              onClick={() => void useRules.getState().saveEdit()}
            >
              Save rules
            </Button>
            <Button size="sm" variant="ghost" onClick={() => useRules.getState().cancelEdit()}>
              Cancel
            </Button>
          </span>
        </Banner>
      )}
      {!enabled && (
        <Banner>
          <span>
            Filters are set aside.
            {kept > 0 && ` ${kept} kept.`}
          </span>
          <span className="flex">
            <Button size="sm" onClick={() => setEnabled(true)}>
              Turn on
            </Button>
          </span>
        </Banner>
      )}

      <ScrollArea className="min-h-0 flex-1">
        <div className={cn("px-3 pt-2 pb-3", !enabled && "opacity-45")}>
          <RuleBuilder />
        </div>
      </ScrollArea>

      {editing === null && (
        <div className="flex items-center gap-2 border-t border-border px-3 py-2">
          <Tooltip
            content={finished ? "Keep these filters as a playlist" : "Set a filter first"}
            side="top"
          >
            {/* A span keeps the tooltip working while the button is disabled. */}
            <span>
              <Button size="sm" disabled={!finished} onClick={() => setSaveOpen(true)}>
                Save as smart playlist…
              </Button>
            </span>
          </Tooltip>
        </div>
      )}
      <SaveSmartDialog open={saveOpen} onOpenChange={setSaveOpen} />
    </aside>
  );
}
