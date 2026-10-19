/**
 * The inside of a divider row in the track table (D22): a rule across the
 * full row width with the label near its start, or the rename input. Label rules are
 * reks-core's; a label that breaks them comes back as the playlist error.
 */
import type { DividerRow } from "../lib/ipc";
import { usePlaylists } from "../store/playlists";
import { Badge } from "./ui/Badge";
import { Input } from "./ui/Input";

const RULE = "h-px bg-border-strong";

export function DividerLabel({ divider, draft }: { divider: DividerRow; draft: string | null }) {
  const p = usePlaylists.getState;
  if (draft !== null) {
    return (
      <div className="flex flex-1 items-center gap-2 px-1.5">
        <Input
          size="xs"
          className="flex-1"
          aria-label="Divider label"
          placeholder="---- PEAK ----"
          value={draft}
          // Rename-in-place starts typing at once.
          autoFocus
          onFocus={(e) => e.currentTarget.select()}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
          onChange={(e) => p().editDividerDraft(e.target.value)}
          onBlur={() => void p().commitDividerRename()}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") void p().commitDividerRename();
            else if (e.key === "Escape") p().cancelDividerRename();
          }}
        />
      </div>
    );
  }
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2 px-1.5 text-xs font-semibold text-fg-muted">
      <span className={`${RULE} w-4 flex-none`} />
      <span className="truncate">{divider.label}</span>
      {!divider.ascii && (
        <Badge
          variant="muted"
          title="Characters other than plain ASCII may not show correctly on players (untested)"
        >
          UNVERIFIED
        </Badge>
      )}
      <span className={`${RULE} min-w-4 flex-1`} />
    </div>
  );
}
