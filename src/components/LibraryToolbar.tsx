/**
 * List toolbar above the track table: search, the Filters button (shows the
 * Filters drawer; badge = rows that filter, tooltip = what they say) with the
 * on/off switch while any is set,
 * track count, background analysis progress, column chooser, and the Track
 * info button. Library-wide actions (scan, import) live in the app header.
 */
import { useMemo } from "react";

import { useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { usePlaylists } from "../store/playlists";
import { useRules } from "../store/rules";
import { activeRowCount, type SummaryNames, summarize } from "../store/ruleTree";
import { useKeyLabel } from "../store/settings";
import { tagIndex } from "../store/tagModel";
import { useTags } from "../store/tags";
import { AnalysisIndicator } from "./AnalysisIndicator";
import { ColumnChooser } from "./ColumnChooser";
import { FiltersSwitch } from "./FilterDrawer";
import { FilterIcon, InfoIcon, TagIcon } from "./icons";
import { SearchBox } from "./SearchBox";
import type { TrackTableModel } from "./trackTableModel";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { Separator } from "./ui/Separator";
import { Tooltip } from "./ui/Tooltip";

/** Names for the Filters summary, looked up where they live. */
function useSummaryNames(): SummaryNames {
  const tree = useTags((s) => s.tree);
  const nodes = usePlaylists((s) => s.nodes);
  const keyLabel = useKeyLabel();
  return useMemo(() => {
    const tags = tree === null ? null : tagIndex(tree);
    return {
      tag: (id) => tags?.get(id)?.tag.name ?? `tag ${id}`,
      category: (id) => tree?.categories.find((c) => c.id === id)?.name ?? "tag",
      playlist: (id) => nodes.find((n) => n.id === id)?.name ?? `playlist ${id}`,
      key: keyLabel,
    };
  }, [tree, nodes, keyLabel]);
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
      <Tooltip
        content={
          <>
            {filterBarOpen ? "Hide filters" : "Show filters"}
            {summary !== null && <span className="block max-w-72 truncate">{summary}</span>}
          </>
        }
      >
        <Button size="sm" variant="ghost" aria-pressed={filterBarOpen} onClick={toggleFilterBar}>
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
      {activeFilters > 0 && <FiltersSwitch />}
      <span className="whitespace-nowrap text-xs text-fg-muted tabular-nums">
        {total.toLocaleString()} tracks
        {selected > 0 ? ` · ${selected.toLocaleString()} selected` : ""}
      </span>
      <span className="flex-1" />
      <AnalysisIndicator />
      <ColumnChooser table={table} />
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
