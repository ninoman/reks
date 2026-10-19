/** Menu with a check item per column. */
import type { TrackTableModel } from "./trackTableModel";
import { Button } from "./ui/Button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";

export function ColumnChooser({ table }: { table: TrackTableModel }) {
  const columns = table.getAllLeafColumns().filter((c) => c.id !== "status");
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="ghost">
          Columns
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        {columns.map((c) => (
          <DropdownMenuCheckboxItem
            key={c.id}
            checked={c.getIsVisible()}
            onCheckedChange={(checked) => c.toggleVisibility(checked)}
            // Keep the menu open to switch several columns.
            onSelect={(e) => e.preventDefault()}
          >
            {String(c.columnDef.header ?? c.id)}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
