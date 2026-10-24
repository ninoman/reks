/** The grid's tempo sections as buttons: tempo and start; one is selected. */
import { formatBpm, formatPosition } from "../../lib/format";
import type { SectionView } from "../../lib/gridView";
import { Button } from "../ui/Button";

export function SectionList({
  sections,
  selected,
  onSelect,
}: {
  sections: SectionView[];
  selected: number | null;
  onSelect: (index: number) => void;
}) {
  return (
    <fieldset className="m-0 flex flex-wrap items-center gap-1 border-0 p-0">
      <legend className="sr-only">Tempo sections</legend>
      {sections.map((s) => (
        <Button
          key={s.index}
          size="xs"
          variant="outline"
          aria-pressed={s.index === selected}
          aria-label={`Section ${s.index + 1}: ${formatBpm(s.bpmX100)} BPM from ${formatPosition(s.startMs)}`}
          onClick={() => onSelect(s.index)}
        >
          <span className="tabular-nums">{formatBpm(s.bpmX100)}</span>
          <span className="text-fg-muted tabular-nums">{formatPosition(s.startMs)}</span>
        </Button>
      ))}
    </fieldset>
  );
}
