/**
 * The advanced filter, under the filter bar: a tree of "match all / any /
 * none of these" groups and conditions, ANDed with the bar. Edits apply as
 * you make them (no Apply button); a live line says what the conditions mean
 * and how many tracks match. Unfinished rows are left out until finished.
 * With a smart playlist's rules loaded, it saves back to that playlist.
 */
import { useMemo, useState } from "react";

import { cn } from "../lib/cn";
import { useLibrary } from "../store/library";
import { usePlaylists } from "../store/playlists";
import { useRules } from "../store/rules";
import { isBlank, rowCount, type SummaryNames, summarize, toRule } from "../store/ruleTree";
import { useKeyLabel } from "../store/settings";
import { tagIndex } from "../store/tagModel";
import { useTags } from "../store/tags";
import { PlusIcon } from "./icons";
import { RuleGroup } from "./RuleGroup";
import { SaveSmartDialog } from "./SaveSmartDialog";
import { Button } from "./ui/Button";
import { Tooltip } from "./ui/Tooltip";

/** Names for the summary line, looked up where they live. */
function useSummaryNames(): SummaryNames {
  const tree = useTags((s) => s.tree);
  const nodes = usePlaylists((s) => s.nodes);
  const keyLabel = useKeyLabel();
  return useMemo(() => {
    const tags = tree === null ? null : tagIndex(tree);
    return {
      tag: (id) => tags?.get(id)?.tag.name ?? `tag ${id}`,
      playlist: (id) => nodes.find((n) => n.id === id)?.name ?? `playlist ${id}`,
      key: keyLabel,
    };
  }, [tree, nodes, keyLabel]);
}

export function RuleBuilder() {
  const root = useRules((s) => s.root);
  const editing = useRules((s) => s.editing);
  const focusId = useRules((s) => s.focusId);
  const actions = useRules.getState();
  const facets = useLibrary((s) => s.facets);
  const nodes = usePlaylists((s) => s.nodes);
  const enabled = useLibrary((s) => s.filtersEnabled);
  const total = useLibrary((s) => s.total);
  const loading = useLibrary((s) => s.pagesGeneration !== s.generation);
  const names = useSummaryNames();
  const [saveOpen, setSaveOpen] = useState(false);

  const summary = summarize(root, names);
  const finished = toRule(root) !== null;
  const rows = rowCount(root);

  return (
    <section
      id="rule-builder"
      aria-label="Advanced filter"
      className="flex shrink-0 flex-col gap-2 border-b border-border bg-bg-subtle px-2.5 py-2"
    >
      {editing !== null && (
        <div className="flex flex-wrap items-center gap-2 rounded-md bg-surface px-2 py-1 text-sm">
          <span className="min-w-0 flex-1">
            Editing the rules of <strong className="font-semibold">{editing.name}</strong>. The
            table shows your whole library as a preview.
          </span>
          <Button
            size="sm"
            variant="primary"
            disabled={!finished}
            onClick={() => void actions.saveEdit()}
          >
            Save rules
          </Button>
          <Button size="sm" variant="ghost" onClick={() => actions.cancelEdit()}>
            Cancel
          </Button>
        </div>
      )}

      <p className="m-0 text-sm text-fg-muted" aria-live="polite" data-testid="rule-summary">
        {summary === null ? (
          "Add a condition to narrow the list. Finished conditions apply at once."
        ) : (
          <>
            <span className="text-fg">{summary}</span>
            {" → "}
            <span className={cn("tabular-nums", loading && "opacity-60")}>
              {total.toLocaleString()} {total === 1 ? "track" : "tracks"}
            </span>
            {!enabled && " (filters are set aside)"}
          </>
        )}
      </p>

      <div className={cn(!enabled && "opacity-45")}>
        {root.rules.length === 0 ? (
          <div className="flex items-center gap-2 text-sm text-fg-muted">
            No conditions yet.
            <Button size="xs" variant="outline" onClick={() => actions.addRow(root.id)}>
              <PlusIcon />
              Add condition
            </Button>
          </div>
        ) : (
          <RuleGroup
            group={root}
            root={root}
            choices={{
              genres: facets?.genres ?? [],
              colours: facets?.colours ?? [],
              playlists: nodes,
            }}
            actions={actions}
            focusId={focusId}
            onFocused={actions.clearFocus}
            nested={false}
            totalRows={rows}
          />
        )}
      </div>

      {editing === null && (
        <div className="flex items-center gap-2">
          <Tooltip
            content={finished ? "Keep these conditions as a playlist" : "Finish a condition first"}
            side="top"
          >
            {/* A span keeps the tooltip working while the button is disabled. */}
            <span>
              <Button
                size="sm"
                variant="secondary"
                disabled={!finished}
                onClick={() => setSaveOpen(true)}
              >
                Save as smart playlist…
              </Button>
            </span>
          </Tooltip>
          <Button size="sm" variant="link" disabled={isBlank(root)} onClick={() => actions.clear()}>
            Clear conditions
          </Button>
        </div>
      )}
      <SaveSmartDialog open={saveOpen} onOpenChange={setSaveOpen} />
    </section>
  );
}
