/** The sidebar's and drawers' dragged-to widths as `--sidebar-w` / `--sheet-w` overrides. */
import type { CSSProperties } from "react";

import { usePanels } from "../store/panels";

/** Inline style for the app's root element; empty while both are at their defaults. */
export function usePanelWidthVars(): CSSProperties {
  const sidebar = usePanels((s) => s.sidebarWidth);
  const sheet = usePanels((s) => s.sheetWidth);
  return {
    ...(sidebar === null ? {} : { "--sidebar-w": `${sidebar}px` }),
    ...(sheet === null ? {} : { "--sheet-w": `${sheet}px` }),
  } as CSSProperties;
}
