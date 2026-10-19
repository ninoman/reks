/** Popover with a checkbox per column. */
import { useState } from "react";

import type { TrackTableModel } from "./trackTableModel";

export function ColumnChooser({ table }: { table: TrackTableModel }) {
  const [open, setOpen] = useState(false);
  const columns = table.getAllLeafColumns().filter((c) => c.id !== "status");
  return (
    <div className="column-chooser">
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
        Columns
      </button>
      {open && (
        <div className="popover" role="menu">
          {columns.map((c) => (
            <label key={c.id}>
              <input
                type="checkbox"
                checked={c.getIsVisible()}
                onChange={(e) => c.toggleVisibility(e.target.checked)}
              />
              {String(c.columnDef.header ?? c.id)}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
