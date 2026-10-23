/** Column model for the track table (react-table): visibility and header order. */
import {
  type ColumnVisibilityState,
  columnVisibilityFeature,
  createColumnHelper,
  tableFeatures,
  type Updater,
  useTable,
} from "@tanstack/react-table";

import type { RowDensity } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { COLUMN_BY_ID, COLUMNS, type ColumnSpec } from "./columns";

export const ROW_HEIGHT = 26;
/** Track table row heights (tokens.css `--row-h`, `--row-h-tall`). */
export const ROW_HEIGHTS: Record<RowDensity, number> = { compact: ROW_HEIGHT, default: 42 };

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

/**
 * Column widths that fill `available` px. Spare width goes to the `grow`
 * columns in proportion to their width (to all columns if none grow); base
 * widths are minimums, so a wide table keeps them and scrolls.
 */
export function fitColumnWidths(
  columns: readonly Pick<ColumnSpec, "width" | "grow">[],
  available: number,
) {
  const base = columns.map((c) => c.width);
  const spare = Math.floor(available) - base.reduce((a, b) => a + b, 0);
  if (spare <= 0) return base;
  const anyGrow = columns.some((c) => c.grow);
  const weights = columns.map((c) => (anyGrow && c.grow !== true ? 0 : c.width));
  const totalWeight = weights.reduce((a, b) => a + b, 0);
  if (totalWeight === 0) return base;
  const shares = weights.map((w) => Math.floor((spare * w) / totalWeight));
  // Rounding remainder goes to the last growing column so the sum is exact.
  const lastGrowing = weights.length - 1 - [...weights].reverse().findIndex((w) => w > 0);
  shares[lastGrowing] = (shares[lastGrowing] ?? 0) + spare - shares.reduce((a, b) => a + b, 0);
  return base.map((w, i) => w + (shares[i] ?? 0));
}
