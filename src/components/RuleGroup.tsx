/**
 * One level of the Filters drawer's query: its items on a rail, joined by
 * "and" / "or" connectors (a click switches the whole level), and nested
 * groups as outlined cards with a header menu (Any / All / None of these,
 * Ungroup, Remove group) and "+ Or…". Presentational.
 */
import { Fragment, useState } from "react";

import { cn } from "../lib/cn";
import type { RowFocus } from "../store/rules";
import {
  canOr,
  FIELD_LABELS,
  type FieldKey,
  fieldKey,
  type GroupDraft,
  type GroupMode,
  type ItemDraft,
  isRowActive,
  type RowDraft,
} from "../store/ruleTree";
import { FieldChooser } from "./FieldChooser";
import { ChevronDownIcon, PlusIcon } from "./icons";
import { RuleRow, type RuleRowActions } from "./RuleRow";
import type { RuleChoices } from "./RuleValueEditor";
import { Button } from "./ui/Button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";
import { Tooltip } from "./ui/Tooltip";

export interface RuleLevelActions extends RuleRowActions {
  setMode(groupId: string, mode: GroupMode): void;
  toggleMode(groupId: string): void;
  ungroup(groupId: string): void;
  addField(groupId: string, key: FieldKey): void;
}

export interface RuleLevelProps {
  group: GroupDraft;
  /** The whole tree: how deep a group is, where Or… may go. */
  root: GroupDraft;
  /** Root rows that don't show (`hiddenRows`). */
  hidden: ReadonlySet<string>;
  /** The default fields (for "Keep in default fields" and × on root rows). */
  defaults: readonly FieldKey[];
  choices: RuleChoices;
  actions: RuleLevelActions;
  focus: RowFocus | null;
}

const MODE_TEXT: Record<GroupMode, string> = {
  all: "All of these",
  any: "Any of these",
  none: "None of these",
};

function itemActive(item: ItemDraft): boolean {
  return item.kind === "row" ? isRowActive(item) : item.rules.some(itemActive);
}

function rowLabel(row: RowDraft, choices: RuleChoices): string | null {
  const v = row.value;
  if (v.field === null) return null;
  if (v.field !== "tag") return FIELD_LABELS[v.field];
  return choices.tags?.categories.find((c) => c.id === v.category)?.name ?? "My Tags";
}

/** "and" / "or" between two items: a click switches the whole level. */
function Connector({ mode, on, onToggle }: { mode: GroupMode; on: boolean; onToggle: () => void }) {
  const or = mode !== "all";
  return (
    <div className="relative h-4">
      <Tooltip
        side="left"
        content={
          or
            ? "Click to match all of these instead of any (applies to this whole level)"
            : "Click to match any of these instead of all (applies to this whole level)"
        }
      >
        <button
          type="button"
          aria-label={or ? "Match all of these instead" : "Match any of these instead"}
          className={cn(
            "absolute top-1/2 left-[calc(var(--query-rail-w)/-2)] -translate-x-1/2 -translate-y-1/2",
            "cursor-default rounded-full border border-border bg-bg-subtle px-1 text-2xs leading-tight font-bold text-fg-subtle uppercase",
            "hover:border-accent hover:text-accent focus-visible:outline-solid focus-visible:outline-(length:--focus-ring-width) focus-visible:outline-focus-ring",
            on && "border-fg-muted bg-surface text-fg",
            or && "border-accent text-accent",
          )}
          onClick={onToggle}
        >
          {or ? "or" : "and"}
        </button>
      </Tooltip>
    </div>
  );
}

/** A nested group: an outlined card with its own level inside. */
function NestedGroup(props: RuleLevelProps) {
  const { group, actions, choices } = props;
  const [chooserOpen, setChooserOpen] = useState(false);
  return (
    // biome-ignore lint/a11y/useSemanticElements: a fieldset's legend and box rules don't fit the card
    <div
      role="group"
      aria-label={`Group: ${MODE_TEXT[group.mode].toLowerCase()}`}
      className="relative flex flex-col gap-0.5 rounded-md border border-accent bg-surface px-1 pt-0.5 pb-1"
    >
      <span
        aria-hidden
        className="absolute top-3 left-[calc(var(--query-rail-w)*-1)] flex w-(--query-rail-w) justify-center"
      >
        <span className="size-(--query-dot) rounded-full bg-accent" />
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button size="xs" variant="ghost" className="self-start text-fg-muted">
            {MODE_TEXT[group.mode]}
            <ChevronDownIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          {(["any", "all", "none"] as const).map((m) => (
            <DropdownMenuCheckboxItem
              key={m}
              checked={group.mode === m}
              onCheckedChange={() => actions.setMode(group.id, m)}
            >
              {MODE_TEXT[m]}
            </DropdownMenuCheckboxItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem inset onSelect={() => actions.ungroup(group.id)}>
            Ungroup
          </DropdownMenuItem>
          <DropdownMenuItem inset variant="danger" onSelect={() => actions.remove(group.id)}>
            Remove group
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {group.rules.length === 0 ? (
        <p className="m-0 px-1.5 text-sm text-fg-muted">No filters in this group yet.</p>
      ) : (
        <RuleLevel {...props} />
      )}
      <div className="pl-(--query-rail-w)">
        <FieldChooser
          trigger={
            <Button size="xs" variant="ghost" className="text-fg-muted">
              <PlusIcon />
              {group.mode === "all" ? "And…" : "Or…"}
            </Button>
          }
          open={chooserOpen}
          onOpenChange={setChooserOpen}
          shown={new Set()}
          categories={choices.tags?.categories ?? null}
          tagsStatus={choices.tagsStatus}
          onPick={(key) => actions.addField(group.id, key)}
        />
      </div>
    </div>
  );
}

export function RuleLevel(props: RuleLevelProps) {
  const { group, root, hidden, defaults, choices, actions, focus } = props;
  const items = group.rules.filter((i) => !hidden.has(i.id));
  return (
    <div
      className={cn(
        "relative flex flex-col pl-(--query-rail-w)",
        "before:absolute before:top-4 before:bottom-4 before:left-[calc(var(--query-rail-w)/2)] before:border-l before:border-border",
      )}
    >
      {items.map((item, i) => {
        const prev = items[i - 1];
        const key = item.kind === "row" ? fieldKey(item.value) : null;
        const isDefault = key !== null && defaults.includes(key);
        return (
          <Fragment key={item.id}>
            {prev !== undefined && (
              <Connector
                mode={group.mode}
                on={itemActive(prev) && itemActive(item)}
                onToggle={() => actions.toggleMode(group.id)}
              />
            )}
            {item.kind === "row" ? (
              <RuleRow
                row={item}
                label={rowLabel(item, choices)}
                isTag={item.value.field === "tag"}
                choices={choices}
                actions={actions}
                focus={focus?.id === item.id ? focus : null}
                isDefault={isDefault}
                keepsPlace={group === root && isDefault}
                canOr={canOr(root, item.id)}
              />
            ) : (
              <NestedGroup {...props} group={item} hidden={new Set()} />
            )}
          </Fragment>
        );
      })}
    </div>
  );
}
