/**
 * Above the table while a smart playlist is shown: says what it is ("Smart"),
 * that it follows its rules, and offers Edit rules. Tracks can't be added,
 * removed or reordered by hand, and the badge's tooltip says why.
 */
import { useLibrary } from "../store/library";
import { shownPlaylist, usePlaylists } from "../store/playlists";
import { useRules } from "../store/rules";
import { SmartIcon } from "./icons";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Tooltip } from "./ui/Tooltip";

export function SmartListHeader() {
  const nodes = usePlaylists((s) => s.nodes);
  const shown = useLibrary((s) => s.playlistId);
  const node = shownPlaylist(nodes, shown);
  if (node === null || !node.smart) return null;

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-bg-subtle px-2.5 py-1 text-sm">
      <Tooltip content="Its tracks follow its rules, so you can't add, remove or reorder them by hand.">
        <Badge size="md" variant="neutral" className="gap-1">
          <SmartIcon />
          Smart
        </Badge>
      </Tooltip>
      <span className="font-semibold">{node.name}</span>
      <span className="text-fg-muted">Updates automatically ·</span>
      <Button
        size="xs"
        variant="link"
        className="text-accent"
        onClick={() => void useRules.getState().editSmart(node)}
      >
        Edit rules
      </Button>
    </div>
  );
}
