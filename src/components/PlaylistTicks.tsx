/**
 * The library's playlists with the selected stick's ticks (its profile,
 * D27), saved as they change. A ticked folder covers (and locks) what's
 * under it. Used by the Sticks view and the export's Playlists step.
 */
import { usePlaylists } from "../store/playlists";
import { coveredIds, useProfiles } from "../store/profiles";
import { Badge } from "./ui/Badge";
import { Card } from "./ui/Card";
import { Checkbox } from "./ui/Checkbox";
import { Progress } from "./ui/Progress";

/** Indent per tree level, as in the sidebar tree. */
const INDENT = 18;

export function PlaylistTicks({
  frees,
  title = "Playlists on this stick",
}: {
  /** "Unticking frees X MB" per ticked entry, when the status is known. */
  frees?: ReadonlyMap<number | null, string> | undefined;
  title?: string;
}) {
  const nodes = usePlaylists((p) => p.nodes);
  const ticks = useProfiles((p) => p.ticks);
  if (nodes.length === 0) {
    return <p className="m-0 text-sm text-fg-muted">The library has no playlists yet.</p>;
  }
  if (ticks === null) return <Progress aria-label="Reading the stick's playlists" />;
  const covered = coveredIds(nodes, ticks);
  return (
    <div className="flex min-h-0 flex-col gap-1">
      <h4 className="m-0 shrink-0 text-xs font-semibold text-fg-muted">{title}</h4>
      <Card role="group" aria-label={title} divided={false} className="scroll-pane min-h-0 py-1">
        {nodes.map((n) => {
          const implied = covered.has(n.id);
          return (
            <label
              key={n.id}
              className="flex h-(--row-h) items-center gap-2 pr-2 text-md"
              style={{ paddingLeft: 8 + n.depth * INDENT }}
              title={
                n.isFolder ? "A folder takes every playlist under it, also later ones" : undefined
              }
            >
              <Checkbox
                checked={implied || ticks.has(n.id)}
                disabled={implied}
                onChange={() => void useProfiles.getState().toggleTick(n.id, nodes)}
              />
              <span className="flex-1 truncate">{n.name}</span>
              {n.smart && (
                <Badge variant="muted" title="Written as a fixed list of its current tracks">
                  Smart
                </Badge>
              )}
              <span className="text-xs text-fg-subtle tabular-nums">
                {frees?.get(n.id) ?? (n.isFolder ? "folder" : n.entries)}
              </span>
            </label>
          );
        })}
      </Card>
    </div>
  );
}
