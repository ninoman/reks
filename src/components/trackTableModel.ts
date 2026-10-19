/** Column model for the track table (react-table): visibility and header order. */
import {
  type ColumnVisibilityState,
  columnVisibilityFeature,
  createColumnHelper,
  tableFeatures,
  type Updater,
  useTable,
} from "@tanstack/react-table";

import { useLibrary } from "../store/library";
import { COLUMN_BY_ID, COLUMNS, type ColumnSpec } from "./columns";

export const ROW_HEIGHT = 26;

const features = tableFeatures({ columnVisibilityFeature });
const helper = createColumnHelper<typeof features, never>();
const columnDefs = COLUMNS.map((c) => helper.display({ id: c.id, header: c.header }));
const NO_DATA: never[] = [];

/** Shared table instance for the header, body and column chooser. */
export function useTrackTable() {
  const columnVisibility = useLibrary((s) => s.columnVisibility);
  const setColumnVisibility = useLibrary((s) => s.setColumnVisibility);
  return useTable({
    features,
    columns: columnDefs,
    data: NO_DATA,
    state: { columnVisibility },
    onColumnVisibilityChange: (u: Updater<ColumnVisibilityState>) =>
      setColumnVisibility(typeof u === "function" ? u(useLibrary.getState().columnVisibility) : u),
  });
}

export type TrackTableModel = ReturnType<typeof useTrackTable>;

/** Visible column specs, in table order. */
export function visibleColumns(table: TrackTableModel): ColumnSpec[] {
  return table
    .getVisibleLeafColumns()
    .map((c) => COLUMN_BY_ID.get(c.id))
    .filter((c): c is ColumnSpec => c !== undefined);
}
