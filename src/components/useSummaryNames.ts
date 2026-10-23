/** Names for the Filters summary and suggested names, looked up where they live. */
import { useMemo } from "react";

import { usePlaylists } from "../store/playlists";
import type { SummaryNames } from "../store/ruleTree";
import { useKeyLabel } from "../store/settings";
import { tagIndex } from "../store/tagModel";
import { useTags } from "../store/tags";

export function useSummaryNames(): SummaryNames {
  const tree = useTags((s) => s.tree);
  const nodes = usePlaylists((s) => s.nodes);
  const keyLabel = useKeyLabel();
  return useMemo(() => {
    const tags = tree === null ? null : tagIndex(tree);
    return {
      tag: (id) => tags?.get(id)?.tag.name ?? `tag ${id}`,
      category: (id) => tree?.categories.find((c) => c.id === id)?.name ?? "tag",
      playlist: (id) => nodes.find((n) => n.id === id)?.name ?? `playlist ${id}`,
      key: keyLabel,
    };
  }, [tree, nodes, keyLabel]);
}
