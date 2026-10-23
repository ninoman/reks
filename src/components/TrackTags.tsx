/**
 * The track's My Tags in the info panel: removable chips and an "Edit tags"
 * button that opens the My Tags drawer over the panel (attach, add, rename,
 * delete). Tags come from the loaded table row, so the chips follow edits made
 * from the context menu too. A chip's name is looked up in the tag tree.
 */
import { useLibrary } from "../store/library";
import { useRules } from "../store/rules";
import { tagIndex } from "../store/tagModel";
import { useTags } from "../store/tags";
import { TagIcon } from "./icons";
import { Button } from "./ui/Button";
import { Chip } from "./ui/Chip";

export function TrackTags({ trackId, fallback }: { trackId: number; fallback: number[] }) {
  const tree = useTags((s) => s.tree);
  const status = useTags((s) => s.status);
  const rowTags = useLibrary((s) => {
    for (const rows of s.pages.values()) {
      const row = rows.find((r) => r.id === trackId);
      if (row !== undefined) return row.tags;
    }
    return undefined;
  });
  const tags = rowTags ?? fallback;
  const names = tree === null ? null : tagIndex(tree);
  const { unassign, openManager } = useTags.getState();

  return (
    <section aria-label="My Tags">
      <h3 className="mt-0 mb-1 text-sm font-semibold">My Tags</h3>
      <div className="flex flex-wrap items-center gap-1">
        {status === "error" && tree === null && (
          <span className="text-xs text-danger">Tags could not be loaded.</span>
        )}
        {tags.map((id) => {
          const hit = names?.get(id);
          if (hit === undefined) return null;
          return (
            <Chip
              key={id}
              title={hit.category.name}
              removeLabel={hit.tag.name}
              activateLabel={`Filter by tag ${hit.tag.name}`}
              onActivate={() =>
                useRules
                  .getState()
                  .addCondition(
                    { field: "tag", category: hit.category.id, ids: [id], all: false },
                    false,
                  )
              }
              onRemove={() => void unassign([trackId], [id])}
            >
              {hit.tag.name}
            </Chip>
          );
        })}
        {tags.length === 0 && tree !== null && (
          <span className="text-xs text-fg-muted">No tags yet.</span>
        )}
        <Button size="xs" variant="outline" onClick={openManager}>
          <TagIcon />
          Edit tags
        </Button>
      </div>
    </section>
  );
}
