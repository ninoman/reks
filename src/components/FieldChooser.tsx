/**
 * The field list behind "+ Add filter", "+ Or…" and a row waiting for its
 * field: fields by group, each My Tags category a field of its own. A field
 * that already shows on the level is greyed "shown"; picking it jumps to its
 * row (the store decides). Presentational. The value search (phase 1 task 8)
 * goes in front of this list and picks through the same `onPick`.
 */
import type { ReactElement } from "react";

import type { TagCategory } from "../lib/ipc";
import { FIELD_GROUPS, FIELD_LABELS, type FieldKey, tagKey } from "../store/ruleTree";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/DropdownMenu";
import type { OptionsStatus } from "./ValuePicker";

export function FieldChooser({
  trigger,
  open,
  onOpenChange,
  shown,
  categories,
  tagsStatus,
  onPick,
  onResetDefaults,
}: {
  trigger: ReactElement;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Fields that show on this level already. */
  shown: ReadonlySet<FieldKey>;
  categories: readonly TagCategory[] | null;
  tagsStatus: OptionsStatus;
  onPick: (key: FieldKey) => void;
  /** Offers "Reset default fields" (the root level's Add filter). */
  onResetDefaults?: () => void;
}) {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        className="max-h-(--radix-dropdown-menu-content-available-height) overflow-y-auto"
        // The picked row takes focus (and opens its picker), not the trigger.
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        {FIELD_GROUPS.map((group) => {
          const items =
            group.fields === "tags"
              ? (categories ?? []).map((c) => ({ key: tagKey(c.id), label: `# ${c.name}` }))
              : group.fields.map((f) => ({ key: f as FieldKey, label: FIELD_LABELS[f] }));
          return (
            <DropdownMenuGroup key={group.label}>
              <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
              {group.fields === "tags" && tagsStatus.state === "loading" && (
                <DropdownMenuItem disabled>Loading tags…</DropdownMenuItem>
              )}
              {group.fields === "tags" && tagsStatus.state === "error" && (
                <DropdownMenuItem onSelect={tagsStatus.retry}>
                  Couldn't load tags · Retry
                </DropdownMenuItem>
              )}
              {group.fields === "tags" && tagsStatus.state === "ready" && items.length === 0 && (
                <DropdownMenuItem disabled>No tag categories</DropdownMenuItem>
              )}
              {items.map((item) => {
                const isShown = shown.has(item.key);
                return (
                  <DropdownMenuItem
                    key={item.key}
                    className={isShown ? "text-fg-muted" : undefined}
                    onSelect={() => onPick(item.key)}
                  >
                    {item.label}
                    {isShown && <span className="ml-auto text-xs text-fg-subtle">shown</span>}
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuGroup>
          );
        })}
        {onResetDefaults !== undefined && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={onResetDefaults}>Reset default fields</DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
