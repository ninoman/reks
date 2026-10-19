/**
 * List toolbar above the track table: search, the Filters button (shows the
 * filter bar; badge = filters set) with the on/off switch while any is set,
 * track count, column chooser, and the Track info button. Library-wide
 * actions (scan, import) live in the app header.
 */

import { activeFilterCount, useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { ColumnChooser } from "./ColumnChooser";
import { FiltersSwitch } from "./FilterBar";
import { FilterIcon, InfoIcon } from "./icons";
import { SearchBox } from "./SearchBox";
import type { TrackTableModel } from "./trackTableModel";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { Separator } from "./ui/Separator";
import { Tooltip } from "./ui/Tooltip";

export function LibraryToolbar({ table }: { table: TrackTableModel }) {
  const total = useLibrary((s) => s.total);
  const selected = useLibrary((s) => s.selection.selected.size);
  const setText = useLibrary((s) => s.setText);
  const activeFilters = useLibrary((s) => activeFilterCount(s.filters));
  const filtersEnabled = useLibrary((s) => s.filtersEnabled);
  const { filterBarOpen, infoOpen, toggleFilterBar, toggleInfo } = usePanels();

  return (
    <div className="flex h-(--toolbar-h) shrink-0 items-center gap-2 border-b border-border px-2.5">
      <SearchBox onSearch={setText} />
      <Tooltip content={filterBarOpen ? "Hide filters" : "Show filters"}>
        <Button size="sm" variant="ghost" aria-pressed={filterBarOpen} onClick={toggleFilterBar}>
          <FilterIcon />
          Filters
          {activeFilters > 0 && (
            <Badge variant={filtersEnabled ? "accent" : "muted"}>{activeFilters}</Badge>
          )}
        </Button>
      </Tooltip>
      {activeFilters > 0 && <FiltersSwitch />}
      <span className="whitespace-nowrap text-xs text-fg-muted tabular-nums">
        {total.toLocaleString()} tracks
        {selected > 0 ? ` · ${selected.toLocaleString()} selected` : ""}
      </span>
      <span className="flex-1" />
      <ColumnChooser table={table} />
      <Separator orientation="vertical" className="my-2.5" />
      <Tooltip content={infoOpen ? "Hide track info" : "Show track info"}>
        <IconButton size="sm" aria-pressed={infoOpen} aria-label="Track info" onClick={toggleInfo}>
          <InfoIcon />
        </IconButton>
      </Tooltip>
    </div>
  );
}
