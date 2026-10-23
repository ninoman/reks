/**
 * A mixer view in outline: Simple is the two channel faders; Advanced adds
 * the crossfader under them and the assign switches beside them.
 */
import type { MixerView } from "../../../lib/ipc";

function Fader() {
  return (
    <span className="relative h-6 w-1 rounded-full bg-border-strong">
      <span className="absolute top-1.5 left-1/2 h-1.5 w-3 -translate-x-1/2 rounded-xs bg-deck-fg" />
    </span>
  );
}

function Assigns() {
  return (
    <span className="flex gap-0.5">
      <span className="size-1 rounded-full bg-deck-fg" />
      <span className="size-1 rounded-full bg-deck-fg" />
    </span>
  );
}

export function MixerViewPreview({ view }: { view: MixerView }) {
  if (view === "simple") {
    return (
      <span className="flex gap-4">
        <Fader />
        <Fader />
      </span>
    );
  }
  return (
    <span className="flex flex-col items-center gap-2">
      <span className="flex items-center gap-3">
        <Assigns />
        <Fader />
        <Fader />
        <Assigns />
      </span>
      <span className="relative h-1 w-12 rounded-full bg-border-strong">
        <span className="absolute top-1/2 left-1/2 h-3 w-1.5 -translate-1/2 rounded-xs bg-deck-fg" />
      </span>
    </span>
  );
}
