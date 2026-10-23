/**
 * The main area's track lists: the main list and, stacked under it, the split
 * panes opened with Shift+click in the playlist tree, sharing the height
 * evenly. Once there is more than one list, each has a header with its name,
 * track count and a close button; closing the main list moves the first pane
 * up into it. The info panel and the My Tags and Filters drawers (on top of it, one at a time) slide over
 * all of them and follow the main list.
 */
import type { ReactNode } from "react";

import { useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { usePanes } from "../store/panes";
import { usePlaylists } from "../store/playlists";
import { useTags } from "../store/tags";
import type { ColumnSpec } from "./columns";
import { EmptyResult } from "./EmptyResult";
import { FilterDrawer } from "./FilterDrawer";
import { CloseIcon } from "./icons";
import { LibraryStoreContext, useLibraryStore } from "./libraryStoreContext";
import { SmartListHeader } from "./SmartListHeader";
import { TagDrawer } from "./TagDrawer";
import { TrackInfoPanel } from "./TrackDetailPanel";
import { TrackTable } from "./TrackTable";
import { IconButton } from "./ui/IconButton";
import { Sheet } from "./ui/Sheet";

export function TrackLists({ columns }: { columns: ColumnSpec[] }) {
  const panes = usePanes((s) => s.panes);
  const infoOpen = usePanels((s) => s.infoOpen);
  const filtersOpen = usePanels((s) => s.filterBarOpen);
  const tagsOpen = useTags((s) => s.managerOpen);
  const nodes = usePlaylists((s) => s.nodes);
  const mainId = useLibrary((s) => s.filters.playlistId);
  const nameOf = (id: number | null) => nodes.find((n) => n.id === id)?.name ?? "Collection";

  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
      <ListPane
        name={nameOf(mainId)}
        columns={columns}
        onClose={panes.length > 0 ? () => usePlaylists.getState().closeMainList() : null}
      >
        <SmartListHeader />
      </ListPane>
      {panes.map((pane) => (
        <LibraryStoreContext.Provider key={pane.key} value={pane.store}>
          <ListPane
            name={nameOf(pane.playlistId)}
            columns={columns}
            onClose={() => usePanes.getState().close(pane.key)}
          />
        </LibraryStoreContext.Provider>
      ))}
      <Sheet open={infoOpen}>
        <TrackInfoPanel />
      </Sheet>
      <Sheet open={tagsOpen}>
        <TagDrawer />
      </Sheet>
      <Sheet open={filtersOpen}>
        <FilterDrawer />
      </Sheet>
    </div>
  );
}

/** One list of the stack; `onClose` `null`: the only list, shown without a header. */
function ListPane({
  name,
  columns,
  onClose,
  children,
}: {
  name: string;
  columns: ColumnSpec[];
  onClose: (() => void) | null;
  children?: ReactNode;
}) {
  const useList = useLibraryStore();
  const total = useList((s) => s.total);
  return (
    <section
      className="relative flex min-h-0 flex-1 flex-col border-border not-first:border-t"
      aria-label={name}
    >
      {onClose !== null && (
        <div className="flex shrink-0 items-center gap-2 border-b border-border bg-bg-subtle py-0.5 pr-1 pl-2.5 text-sm">
          <span className="min-w-0 flex-1 truncate font-semibold">{name}</span>
          <span className="text-fg-muted">{total} tracks</span>
          <IconButton size="xs" aria-label={`Close ${name}`} title="Close" onClick={onClose}>
            <CloseIcon />
          </IconButton>
        </div>
      )}
      {children}
      <div className="relative flex min-h-0 flex-1 flex-col">
        <TrackTable columns={columns} />
        <EmptyResult />
      </div>
    </section>
  );
}
