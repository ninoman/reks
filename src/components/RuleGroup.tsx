/**
 * A group of conditions: "Tracks that match [all | any | none] of these", its
 * rows and nested groups, then "+ Add condition" and "+ Add group". Nested
 * groups hang off an indented left rule. Presentational.
 */
import { canAddGroup, type GroupDraft, type GroupMode, type ItemDraft } from "../store/ruleTree";
import { CloseIcon, PlusIcon } from "./icons";
import { RuleRow, type RuleRowActions } from "./RuleRow";
import type { RuleChoices } from "./RuleValueEditor";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";

export interface RuleGroupActions extends RuleRowActions {
  setMode(groupId: string, mode: GroupMode): void;
  addRow(groupId: string): void;
  addGroup(groupId: string): void;
}

export interface RuleGroupProps {
  group: GroupDraft;
  /** Whole tree's root: needed to know how deep this group is. */
  root: GroupDraft;
  choices: RuleChoices;
  actions: RuleGroupActions;
  focusId: string | null;
  onFocused: () => void;
  /** Nested groups can be removed; the root cannot. */
  nested: boolean;
  /** Rows in the whole builder (the last one can't be removed). */
  totalRows: number;
}

const MODE_WORD: Record<GroupMode, string> = { all: "all", any: "any", none: "none" };

export function RuleGroup({
  group,
  root,
  choices,
  actions,
  focusId,
  onFocused,
  nested,
  totalRows,
}: RuleGroupProps) {
  const render = (item: ItemDraft) =>
    item.kind === "row" ? (
      <RuleRow
        key={item.id}
        row={item}
        choices={choices}
        actions={actions}
        autoFocus={focusId === item.id}
        onFocused={onFocused}
        removable={totalRows > 1}
      />
    ) : (
      <RuleGroup
        key={item.id}
        group={item}
        root={root}
        choices={choices}
        actions={actions}
        focusId={focusId}
        onFocused={onFocused}
        nested
        totalRows={totalRows}
      />
    );

  return (
    <fieldset
      aria-label={`Group: match ${MODE_WORD[group.mode]}`}
      className={
        nested
          ? "m-0 flex min-w-0 flex-col gap-1.5 border-0 border-l-2 border-border-strong py-0.5 pl-3"
          : "m-0 flex min-w-0 flex-col gap-1.5 border-0 p-0"
      }
    >
      <div className="flex items-center gap-2 text-sm text-fg-muted">
        Tracks that match
        <SegmentedControl
          size="sm"
          aria-label="Match"
          value={group.mode}
          onValueChange={(m) => actions.setMode(group.id, m as GroupMode)}
        >
          <Segment value="all">all</Segment>
          <Segment value="any">any</Segment>
          <Segment value="none">none</Segment>
        </SegmentedControl>
        of these
        {nested && (
          <IconButton
            size="xs"
            className="ml-auto"
            aria-label="Remove group"
            title="Remove group"
            onClick={() => actions.remove(group.id)}
          >
            <CloseIcon />
          </IconButton>
        )}
      </div>
      {group.rules.map(render)}
      {group.rules.length === 0 && (
        <p className="m-0 text-sm text-fg-muted">No conditions in this group yet.</p>
      )}
      <div className="flex items-center gap-1">
        <Button size="xs" variant="ghost" onClick={() => actions.addRow(group.id)}>
          <PlusIcon />
          Add condition
        </Button>
        {canAddGroup(root, group.id) && (
          <Button
            size="xs"
            variant="ghost"
            className="text-fg-muted"
            onClick={() => actions.addGroup(group.id)}
          >
            <PlusIcon />
            Add group
          </Button>
        )}
      </div>
    </fieldset>
  );
}
