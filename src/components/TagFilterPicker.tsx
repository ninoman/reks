/**
 * My Tags value picker: a button showing the chosen tags and a popover with
 * the tag picker; "tracks with at least one of these". Used by the filter
 * bar and the rule builder's Tag rows.
 */
import type { TagId } from "../lib/ipc";
import { tagIndex } from "../store/tagModel";
import { useTags } from "../store/tags";
import { ChevronDownIcon } from "./icons";
import { TagPicker } from "./TagPicker";
import { Button } from "./ui/Button";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/Popover";

export function TagFilterPicker({
  ids,
  onChange,
  label = "My Tags",
}: {
  ids: readonly TagId[];
  onChange: (ids: TagId[]) => void;
  label?: string;
}) {
  const tree = useTags((s) => s.tree);

  const names = tree === null ? null : tagIndex(tree);
  const summary =
    ids.length === 0
      ? "Any"
      : ids.length === 1
        ? (names?.get(ids[0] as number)?.tag.name ?? "1 tag")
        : `${ids.length} tags`;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size="sm" className="min-w-16 justify-between font-normal" aria-label={label}>
          {summary}
          <ChevronDownIcon />
        </Button>
      </PopoverTrigger>
      <PopoverContent>
        <TagPicker
          coverageOf={(id) => (ids.includes(id) ? "all" : "none")}
          onToggle={(id, cov) =>
            onChange(cov === "all" ? ids.filter((t) => t !== id) : [...ids, id])
          }
        />
      </PopoverContent>
    </Popover>
  );
}
