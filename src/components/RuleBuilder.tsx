/**
 * The Filters drawer's query (D35): the root level's rows and groups and
 * "+ Add filter". Reads the stores and hands `RuleLevel` the draft, the
 * pickers' choices and the actions; edits apply as they are made (no Apply
 * button), and the table is the feedback (no count or summary here).
 */
import { useMemo, useState } from "react";

import type { KeyInfo } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { usePlaylists } from "../store/playlists";
import { useRules } from "../store/rules";
import { defaultKeys, hiddenRows, shownKeys } from "../store/ruleTree";
import { useKeyLabel } from "../store/settings";
import { useTags } from "../store/tags";
import { FieldChooser } from "./FieldChooser";
import { PlusIcon } from "./icons";
import { RuleLevel } from "./RuleGroup";
import type { RuleChoices } from "./RuleValueEditor";
import { Button } from "./ui/Button";
import type { OptionsStatus } from "./ValuePicker";

const NO_KEYS: readonly KeyInfo[] = [];
const NONE: readonly string[] = [];

/** The pickers' options and their loading / error states, from the stores. */
function useRuleChoices(): RuleChoices {
  const facets = useLibrary((s) => s.facets);
  const facetsError = useLibrary((s) => s.facetsError);
  const playlists = usePlaylists((s) => s.nodes);
  const tags = useTags((s) => s.tree);
  const tagsState = useTags((s) => s.status);
  const tagsError = useTags((s) => s.error);
  const keyLabel = useKeyLabel();

  const facetsStatus: OptionsStatus =
    facets !== null
      ? { state: "ready" }
      : facetsError !== null
        ? {
            state: "error",
            error: facetsError,
            retry: () => void useLibrary.getState().loadFacets(),
          }
        : { state: "loading" };
  const tagsStatus: OptionsStatus =
    tags !== null
      ? { state: "ready" }
      : tagsState === "error"
        ? { state: "error", error: tagsError ?? "", retry: () => void useTags.getState().load() }
        : { state: "loading" };
  return {
    genres: facets?.genres ?? NONE,
    colours: facets?.colours ?? NONE,
    keys: facets?.keys ?? NO_KEYS,
    facets: facetsStatus,
    playlists,
    tags,
    tagsStatus,
    keyLabel,
  };
}

export function RuleBuilder() {
  const root = useRules((s) => s.root);
  const focus = useRules((s) => s.focus);
  const defaultFields = useRules((s) => s.defaultFields);
  const tree = useTags((s) => s.tree);
  const actions = useRules.getState();
  const choices = useRuleChoices();
  const [addOpen, setAddOpen] = useState(false);

  const defaults = useMemo(
    () => defaultKeys(defaultFields, tree?.categories.map((c) => c.id) ?? null),
    [defaultFields, tree],
  );
  const hidden = useMemo(() => hiddenRows(root), [root]);
  const empty = root.rules.every((i) => hidden.has(i.id));

  return (
    <section aria-label="Filter rows" className="flex flex-col gap-1">
      <div data-query>
        {empty ? (
          <p className="m-0 py-1 text-sm text-fg-muted">No filters here. Add one below.</p>
        ) : (
          <RuleLevel
            group={root}
            root={root}
            hidden={hidden}
            defaults={defaults}
            choices={choices}
            actions={actions}
            focus={focus}
          />
        )}
      </div>
      <div className="pl-(--query-rail-w)">
        <FieldChooser
          trigger={
            <Button size="sm" variant="ghost" className="text-fg-muted">
              <PlusIcon />
              Add filter
            </Button>
          }
          open={addOpen}
          onOpenChange={setAddOpen}
          shown={shownKeys(root, root)}
          categories={tree?.categories ?? null}
          tagsStatus={choices.tagsStatus}
          onPick={(key) => actions.addField(root.id, key)}
          onResetDefaults={actions.resetDefaults}
        />
      </div>
    </section>
  );
}
