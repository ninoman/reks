/** A knob with its caption above, as on a mixer's front panel. */
import { Caption } from "../ui/Caption";
import { Knob, type KnobProps } from "../ui/Knob";

export function LabelledKnob({ caption, ...props }: KnobProps & { caption: string }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <Caption aria-hidden="true">{caption}</Caption>
      <Knob {...props} />
    </div>
  );
}
