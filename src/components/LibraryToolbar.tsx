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

export function LibraryToolbar({ table }: { table: TrackTableModel }) {
  const total = useLibrary((s) => s.total);
  const selected = useLibrary((s) => s.selection.selected.size);
  const setText = useLibrary((s) => s.setText);
  const activeFilters = useLibrary((s) => activeFilterCount(s.filters));
  const filtersEnabled = useLibrary((s) => s.filtersEnabled);
  const { filterBarOpen, infoOpen, toggleFilterBar, toggleInfo } = usePanels();

  return (
    <div className="toolbar">
      <SearchBox onSearch={setText} />
      <button
        type="button"
        className={`toggle${filterBarOpen ? " on" : ""}`}
        aria-pressed={filterBarOpen}
        title={filterBarOpen ? "Hide filters" : "Show filters"}
        onClick={toggleFilterBar}
      >
        <FilterIcon />
        Filters
        {activeFilters > 0 && (
          <span className={`badge${filtersEnabled ? "" : " off"}`}>{activeFilters}</span>
        )}
      </button>
      {activeFilters > 0 && <FiltersSwitch />}
      <span className="count">
        {total.toLocaleString()} tracks
        {selected > 0 ? ` · ${selected.toLocaleString()} selected` : ""}
      </span>
      <span className="spacer" />
      <ColumnChooser table={table} />
      <span className="toolbar-divider" />
      <button
        type="button"
        className={`toggle icon${infoOpen ? " on" : ""}`}
        aria-pressed={infoOpen}
        aria-label="Track info"
        title={infoOpen ? "Hide track info" : "Show track info"}
        onClick={toggleInfo}
      >
        <InfoIcon />
      </button>
    </div>
  );
}
