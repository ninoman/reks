/**
 * A deck's tempo column: the range (±6 / ±10 / ±16 / WIDE) and M (master
 * tempo: key lock, lit when on), the
 * CDJ-style tempo fader (faster towards the bottom; snaps at 0%,
 * double-click or Delete resets) and the held nudge arrows.
 */
import {
  formatTempo,
  nextTempoRange,
  type TempoRange,
  tempoFader,
  tempoRangeLabel,
  tempoRatio,
} from "../../lib/mixView";
import { NudgeDownIcon, NudgeUpIcon } from "../icons";
import { Button } from "../ui/Button";
import { Fader } from "../ui/Fader";
import { IconButton } from "../ui/IconButton";
import { holdHandlers } from "./hold";

export interface DeckTempoProps {
  deck: number;
  loaded: boolean;
  tempo: number;
  range: TempoRange;
  masterTempo: boolean;
  /** Scale marks on the fader, ends included: more on a taller deck. */
  ticks: number;
  slowerKey: string | undefined;
  fasterKey: string | undefined;
  onTempo: (ratio: number) => void;
  onRange: (range: TempoRange) => void;
  onMasterTempo: () => void;
  /** -1 slower, 1 faster, 0 released. */
  onNudge: (dir: -1 | 0 | 1) => void;
}

function rangeName(r: TempoRange): string {
  return r === 1 ? "wide" : `${tempoRangeLabel(r)}%`;
}

export function DeckTempo({
  deck,
  loaded,
  tempo,
  range,
  masterTempo,
  ticks,
  slowerKey,
  fasterKey,
  onTempo,
  onRange,
  onMasterTempo,
  onNudge,
}: DeckTempoProps) {
  const n = deck + 1;
  return (
    <div className="flex w-(--deck-tempo-w) shrink-0 flex-col items-center gap-1 border-l border-border pl-2 box-content">
      <div className="flex w-full gap-0.5">
        <Button
          size="xs"
          disabled={!loaded}
          aria-label={`Deck ${n} tempo range ${rangeName(range)}`}
          title="Tempo range"
          className="min-w-0 flex-1 px-0.5 font-mono"
          onClick={() => onRange(nextTempoRange(range))}
        >
          {tempoRangeLabel(range)}
        </Button>
        <Button
          size="xs"
          variant="control"
          disabled={!loaded}
          aria-pressed={masterTempo}
          aria-label={`Deck ${n} master tempo`}
          hint="Master tempo (key lock)"
          className="w-(--ctl-h-xs) px-0"
          onClick={onMasterTempo}
        >
          M
        </Button>
      </div>
      <Fader
        disabled={!loaded}
        value={tempoFader(tempo, range)}
        defaultValue={0.5}
        detent={0.5}
        ticks={ticks}
        onValueChange={(v) => onTempo(tempoRatio(v, range))}
        aria-label={`Deck ${n} tempo`}
        valueText={formatTempo(tempo)}
        className="h-auto min-h-0 flex-1"
      />
      <div className="flex gap-1">
        <IconButton
          size="xs"
          variant="secondary"
          disabled={!loaded}
          aria-label={`Deck ${n} nudge slower`}
          hint="Nudge slower (hold)"
          shortcut={slowerKey}
          {...holdHandlers((down) => onNudge(down ? -1 : 0))}
        >
          <NudgeDownIcon />
        </IconButton>
        <IconButton
          size="xs"
          variant="secondary"
          disabled={!loaded}
          aria-label={`Deck ${n} nudge faster`}
          hint="Nudge faster (hold)"
          shortcut={fasterKey}
          {...holdHandlers((down) => onNudge(down ? 1 : 0))}
        >
          <NudgeUpIcon />
        </IconButton>
      </div>
    </div>
  );
}
