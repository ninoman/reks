/**
 * Edits of the selected tempo section: its BPM (typed, or tapped), ½ and
 * ×2, beat 1 at the cursor, stretch onto the cursor, reset to the analysis,
 * delete its marker. Presentational: every button calls `onEdit`.
 */
import { formatBpm } from "../../lib/format";
import type { GridEdit } from "../../lib/ipc";
import { Button } from "../ui/Button";
import { InlineNumber } from "../ui/InlineNumber";

export function SectionControls({
  section,
  count,
  bpmX100,
  cursorMs,
  tapBpmX100,
  disabled,
  resettable,
  onEdit,
  onTap,
}: {
  /** The selected section's index. */
  section: number;
  /** Sections in the grid. */
  count: number;
  bpmX100: number;
  cursorMs: number | null;
  tapBpmX100: number | null;
  disabled: boolean;
  /** The grid was edited, so there is an analysis grid to go back to. */
  resettable: boolean;
  onEdit: (edit: GridEdit) => void;
  onTap: () => void;
}) {
  const noCursor = disabled || cursorMs === null;
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="text-xs text-fg-muted">
        Section {section + 1} of {count}
      </span>
      <InlineNumber
        className="w-16 font-semibold text-md tabular-nums"
        aria-label="Section BPM"
        title="Double-click or Enter to type a BPM"
        value={bpmX100 / 100}
        format={(v) => v.toFixed(2)}
        disabled={disabled}
        onCommit={(v) => onEdit({ op: "setSectionBpm", section, bpmX100: Math.round(v * 100) })}
      />
      <span className="text-xs text-fg-muted">BPM</span>
      <span className="mx-1 h-4 w-px bg-border" />
      <Button size="sm" variant="secondary" onClick={onTap} disabled={disabled}>
        Tap
      </Button>
      <span className="min-w-12 text-xs tabular-nums" data-testid="tap-bpm">
        {tapBpmX100 === null ? "–" : formatBpm(tapBpmX100)}
      </span>
      <Button
        size="sm"
        variant="outline"
        disabled={disabled || tapBpmX100 === null}
        onClick={() =>
          tapBpmX100 !== null && onEdit({ op: "setSectionBpm", section, bpmX100: tapBpmX100 })
        }
      >
        Use tapped BPM
      </Button>
      <span className="mx-1 h-4 w-px bg-border" />
      <Button
        size="sm"
        variant="outline"
        disabled={disabled}
        onClick={() => onEdit({ op: "halve", section })}
      >
        ½
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={disabled}
        onClick={() => onEdit({ op: "double", section })}
      >
        ×2
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={noCursor}
        onClick={() => cursorMs !== null && onEdit({ op: "setDownbeat", atMs: cursorMs })}
      >
        Downbeat at cursor
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={noCursor}
        title="Retempo the section so its beat nearest the cursor lands on it"
        onClick={() =>
          cursorMs !== null && onEdit({ op: "stretch", section, beatMs: cursorMs, toMs: cursorMs })
        }
      >
        Stretch to cursor
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={disabled || !resettable}
        onClick={() => onEdit({ op: "reset", section })}
      >
        Reset section
      </Button>
      <Button
        size="sm"
        variant="quiet"
        disabled={disabled || count < 2}
        onClick={() => onEdit({ op: "deleteMarker", section })}
      >
        Delete marker
      </Button>
    </div>
  );
}
