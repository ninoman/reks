/** The info panel's frame: a fixed head, then everything else scrolls. */
import type { ReactNode } from "react";

import { usePanels } from "../store/panels";
import { ScrollArea } from "./ui/ScrollArea";
import { SheetHeader } from "./ui/Sheet";

export const infoMuted = "m-0 text-fg-muted";

export function InfoPanel({ children }: { children: ReactNode }) {
  const toggleInfo = usePanels((s) => s.toggleInfo);
  return (
    <aside className="flex min-h-0 flex-1 flex-col" aria-label="Track details">
      <div className="px-3 pt-2">
        <SheetHeader title="Track info" closeLabel="Close track info" onClose={toggleInfo} />
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-2.5 px-3 pt-1 pb-3">{children}</div>
      </ScrollArea>
    </aside>
  );
}
