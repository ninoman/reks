/**
 * Two channel strips of an EQ layout: 4-band has four knobs and the X / Y
 * filters between the strips; 3-band has three knobs and a lit FX knob.
 */
import { cn } from "../../../lib/cn";
import type { MixerEq } from "../../../lib/ipc";

function Strip({ eq }: { eq: MixerEq }) {
  const bands = eq === "fourBand" ? 4 : 3;
  return (
    <span className="flex flex-col items-center gap-0.5">
      {Array.from({ length: bands }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: identical decorative knobs.
        <span key={i} className="size-2 rounded-full border border-deck-fg" />
      ))}
      {eq === "threeBand" && <span className="size-2 rounded-full bg-lit shadow-lit" />}
    </span>
  );
}

function FilterLabel({ name }: { name: string }) {
  return (
    <span className="rounded-xs border border-deck-fg px-0.5 text-2xs leading-none font-semibold">
      {name}
    </span>
  );
}

export function EqPreview({ eq }: { eq: MixerEq }) {
  return (
    <span className={cn("flex items-center", eq === "fourBand" ? "gap-2.5" : "gap-5")}>
      <Strip eq={eq} />
      {eq === "fourBand" && (
        <span className="flex flex-col gap-1.5">
          <FilterLabel name="X" />
          <FilterLabel name="Y" />
        </span>
      )}
      <Strip eq={eq} />
    </span>
  );
}
