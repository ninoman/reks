/** The "My Tags" row under the playlist tree: opens the tag manager. */
import { useTags } from "../store/tags";
import { TagIcon } from "./icons";
import { ICON, NAME, PAD, ROW } from "./treeStyles";
import { focusRingInset } from "./ui/control";

export function TagsSidebarEntry() {
  const openManager = useTags((s) => s.openManager);
  const tags = useTags((s) =>
    s.tree === null ? null : s.tree.categories.reduce((n, c) => n + c.tags.length, 0),
  );
  return (
    <button
      type="button"
      className={`${ROW} w-full shrink-0 border-t border-border text-left ${focusRingInset} hover:bg-hover`}
      style={{ paddingLeft: PAD + 4 }}
      onClick={openManager}
    >
      <span className={ICON}>
        <TagIcon />
      </span>
      <span className={NAME}>My Tags</span>
      {tags !== null && <span className="text-xs text-fg-subtle tabular-nums">{tags}</span>}
    </button>
  );
}
