/** A three-way X / off / Y switch (filter and crossfader assign). */
import type { Assign } from "../../lib/mixView";
import { Segment, SegmentedControl } from "../ui/SegmentedControl";

export function AssignSwitch({
  value,
  onChange,
  label,
}: {
  value: Assign;
  onChange: (v: Assign) => void;
  label: string;
}) {
  return (
    <SegmentedControl
      size="xs"
      aria-label={label}
      value={value}
      onValueChange={(v) => onChange(v as Assign)}
    >
      <Segment value="x" aria-label="X" className="px-1">
        X
      </Segment>
      <Segment value="off" aria-label="Off" className="px-1">
        ·
      </Segment>
      <Segment value="y" aria-label="Y" className="px-1">
        Y
      </Segment>
    </SegmentedControl>
  );
}
