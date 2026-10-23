/** Menu with the row density and a check item per column. */
import { usePanels } from "../store/panels";
import { COLUMN_BY_ID } from "./columns";
import type { TrackTableModel } from "./trackTableModel";
import { Button } from "./ui/Button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";

export function ColumnChooser({ table }: { table: TrackTableModel }) {
  const columns = table.getAllLeafColumns().filter((c) => c.id !== "status");
  const density = usePanels((s) => s.rowDensity);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="ghost">
          Columns
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuCheckboxItem
          checked={density === "compact"}
          onCheckedChange={(compact) =>
            usePanels.getState().setRowDensity(compact ? "compact" : "default")
          }
          onSelect={(e) => e.preventDefault()}
        >
          Compact rows
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        {columns.map((c) => (
          <DropdownMenuCheckboxItem
            key={c.id}
            checked={c.getIsVisible()}
            onCheckedChange={(checked) => c.toggleVisibility(checked)}
            // Keep the menu open to switch several columns.
            onSelect={(e) => e.preventDefault()}
          >
            {String(c.columnDef.header ?? c.id)}
            {density === "default" && COLUMN_BY_ID.get(c.id)?.compactOnly && (
              <span className="text-fg-subtle"> (compact rows)</span>
            )}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
