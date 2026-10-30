/**
 * MASTER (lit while this deck is the sync master; a click takes master) and
 * SYNC, one beside each pad row (`--deck-ctl-rows`), filling the column. Both
 * need a grid. SYNC lights in the synced colour (`data-lit`), matching the
 * BPM readout while it follows the master. The player deck alone has neither.
 * In BPM sync (a nudge or jump left the master's phase) SYNC reads BPM; a
 * press restores beat sync.
 */
import { Button } from "../ui/Button";

export interface DeckSyncButtonsProps {
  deck: number;
  /** Loaded with a grid the engine has. */
  ready: boolean;
  master: boolean;
  sync: boolean;
  bpmSync: boolean;
  masterKey: string | undefined;
  syncKey: string | undefined;
  onMaster: () => void;
  onSync: () => void;
}

export function DeckSyncButtons({
  deck,
  ready,
  master,
  sync,
  bpmSync,
  masterKey,
  syncKey,
  onMaster,
  onSync,
}: DeckSyncButtonsProps) {
  const n = deck + 1;
  return (
    <div className="grid w-(--deck-sync-w) shrink-0 grid-rows-(--deck-ctl-rows) gap-y-1">
      <Button
        size="md"
        variant="control"
        disabled={!ready}
        aria-pressed={master}
        aria-label={`Deck ${n} master`}
        hint={master ? "Master" : "Take master"}
        shortcut={masterKey}
        className="row-start-2 size-full px-0 text-2xs font-bold tracking-caption"
        onClick={onMaster}
      >
        MASTER
      </Button>
      <Button
        size="md"
        variant="control"
        disabled={!ready}
        aria-pressed={sync}
        data-lit="synced"
        aria-label={`Deck ${n} sync`}
        hint={sync && bpmSync ? "BPM sync: tempo only (press for beat sync)" : "Sync"}
        shortcut={syncKey}
        className="row-start-3 size-full px-0 text-2xs font-bold tracking-caption"
        onClick={onSync}
      >
        {sync && bpmSync ? "BPM" : "SYNC"}
      </Button>
    </div>
  );
}
