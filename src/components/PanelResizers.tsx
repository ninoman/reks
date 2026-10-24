/**
 * Drag-to-resize edges for the sidebar and the right-hand drawers (info
 * panel, My Tags, Filters; one shared width). The widths live in the panels
 * store and reach the layout as the `--sidebar-w` / `--sheet-w` tokens.
 */
import { usePanels } from "../store/panels";
import { ResizeHandle } from "./ui/ResizeHandle";

const SIDEBAR = { min: 160, max: 480 };
const SHEET = { min: 260, max: 720 };

/** On the sidebar's right edge; the sidebar must be positioned. */
export function SidebarResizeHandle() {
  const width = usePanels((s) => s.sidebarWidth);
  const set = usePanels((s) => s.setSidebarWidth);
  return (
    <ResizeHandle
      label="Resize sidebar"
      edge="right"
      {...SIDEBAR}
      value={width ?? undefined}
      onResize={set}
      onReset={() => set(null)}
    />
  );
}

/** On a drawer's left edge (inside its `Sheet`). */
export function SheetResizeHandle() {
  const width = usePanels((s) => s.sheetWidth);
  const set = usePanels((s) => s.setSheetWidth);
  return (
    <ResizeHandle
      label="Resize panel"
      edge="left"
      {...SHEET}
      value={width ?? undefined}
      onResize={set}
      onReset={() => set(null)}
    />
  );
}
