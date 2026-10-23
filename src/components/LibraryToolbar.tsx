/**
 * List toolbar above the track table: search and track count on the left;
 * background analysis progress, column chooser, the Filters button (shows the
 * Filters drawer; badge = rows that filter, tooltip = what they say) with its
 * saved-filters menu (↓ on the button opens it) and the on/off switch while any
 * is set, My Tags and the Track info button on the right, beside the drawers
 * they open. Library-wide actions (scan, import) live in the app header.
 */
import { useState } from "react";

import { useFilterPresets } from "../store/filterPresets";
import { useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { useRules } from "../store/rules";
import { activeRowCount, summarize } from "../store/ruleTree";
import { useTags } from "../store/tags";
import { AnalysisIndicator } from "./AnalysisIndicator";
import { ColumnChooser } from "./ColumnChooser";
import { FiltersSwitch } from "./FilterDrawer";
import { ChevronDownIcon, FilterIcon, InfoIcon, TagIcon } from "./icons";
import { SearchBox } from "./SearchBox";
import type { TrackTableModel } from "./trackTableModel";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";
import { IconButton } from "./ui/IconButton";
import { Separator } from "./ui/Separator";
import { Tooltip } from "./ui/Tooltip";
import { useSummaryNames } from "./useSummaryNames";

/** The Filters button's ▾: load a saved filter with the drawer closed (D35, S4). */
function SavedFiltersMenu({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const presets = useFilterPresets((s) => s.presets);
  const status = useFilterPresets((s) => s.status);
  const loaded = useRules((s) => s.preset);
  const { filterBarOpen, toggleFilterBar } = usePanels();
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>
        <IconButton size="sm" aria-label="Saved filters" className="-ml-1.5">
          <ChevronDownIcon />
        </IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {status === "loading" && (
          <DropdownMenuItem disabled>Loading saved filters…</DropdownMenuItem>
        )}
        {status === "error" && (
          <DropdownMenuItem onSelect={() => void useFilterPresets.getState().load()}>
            Couldn't load saved filters · Retry
          </DropdownMenuItem>
        )}
        {status === "ready" && presets.length === 0 && (
          <DropdownMenuItem disabled>No saved filters yet</DropdownMenuItem>
        )}
        {presets.map((p) => (
          <DropdownMenuCheckboxItem
            key={p.id}
            checked={loaded?.id === p.id}
            onSelect={() => useRules.getState().loadPreset(p)}
          >
            {p.name}
          </DropdownMenuCheckboxItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={toggleFilterBar}>
          {filterBarOpen ? "Hide filters" : "Show filters"}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function LibraryToolbar({ table }: { table: TrackTableModel }) {
  const total = useLibrary((s) => s.total);
  const selected = useLibrary((s) => s.selection.selected.size);
  const setText = useLibrary((s) => s.setText);
  const root = useRules((s) => s.root);
  const activeFilters = activeRowCount(root);
  const summary = summarize(root, useSummaryNames());
  const filtersEnabled = useLibrary((s) => s.filtersEnabled);
  const { filterBarOpen, infoOpen, toggleFilterBar, toggleInfo } = usePanels();
  const [savedOpen, setSavedOpen] = useState(false);
  const tagsOpen = useTags((s) => s.managerOpen);
  // The tag drawer covers the info panel: the info button brings info to the front first.
  const infoShown = infoOpen && !tagsOpen;
  function onInfo() {
    if (tagsOpen) useTags.getState().closeManager();
    if (!tagsOpen || !infoOpen) toggleInfo();
  }

  return (
    <div className="flex h-(--toolbar-h) shrink-0 items-center gap-2 border-b border-border px-2.5">
      <SearchBox onSearch={setText} />
      <span className="whitespace-nowrap text-xs text-fg-muted tabular-nums">
        {total.toLocaleString()} tracks
        {selected > 0 ? ` · ${selected.toLocaleString()} selected` : ""}
      </span>
      <span className="flex-1" />
      <AnalysisIndicator />
      <ColumnChooser table={table} />
      <Tooltip
        content={
          <>
            {filterBarOpen ? "Hide filters" : "Show filters"}
            {summary !== null && <span className="block max-w-72 truncate">{summary}</span>}
          </>
        }
      >
        <Button
          size="sm"
          variant="ghost"
          aria-pressed={filterBarOpen}
          aria-keyshortcuts="ArrowDown"
          onClick={toggleFilterBar}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSavedOpen(true);
            }
          }}
        >
          <FilterIcon />
          Filters
          {activeFilters > 0 && (
            <>
              <Badge aria-hidden variant={filtersEnabled ? "accent" : "muted"}>
                {activeFilters}
              </Badge>
              <span className="sr-only">, {activeFilters} active</span>
            </>
          )}
        </Button>
      </Tooltip>
      <SavedFiltersMenu open={savedOpen} onOpenChange={setSavedOpen} />
      {activeFilters > 0 && <FiltersSwitch />}
      <Tooltip content={tagsOpen ? "Hide My Tags" : "Show My Tags"}>
        <IconButton
          size="sm"
          aria-pressed={tagsOpen}
          aria-label="My Tags"
          onClick={() => useTags.getState().toggleManager()}
        >
          <TagIcon />
        </IconButton>
      </Tooltip>
      <Separator orientation="vertical" className="my-2.5" />
      <Tooltip content={infoShown ? "Hide track info" : "Show track info"}>
        <IconButton size="sm" aria-pressed={infoShown} aria-label="Track info" onClick={onInfo}>
          <InfoIcon />
        </IconButton>
      </Tooltip>
    </div>
  );
}
