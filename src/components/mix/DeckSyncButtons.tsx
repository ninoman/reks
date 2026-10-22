/**
 * MASTER (lit while this deck is the sync master; a click takes master) and
 * SYNC. Both need a grid. The player deck alone has neither.
 */
import { Button } from "../ui/Button";

export interface DeckSyncButtonsProps {
  deck: number;
  /** Loaded with a grid the engine has. */
  ready: boolean;
  master: boolean;
  sync: boolean;
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
  masterKey,
  syncKey,
  onMaster,
  onSync,
}: DeckSyncButtonsProps) {
  const n = deck + 1;
  return (
    <div className="ml-auto flex w-(--deck-sync-w) shrink-0 flex-col justify-end gap-2">
      <Button
        size="md"
        variant="control"
        disabled={!ready}
        aria-pressed={master}
        aria-label={`Deck ${n} master`}
        hint={master ? "Master" : "Take master"}
        shortcut={masterKey}
        className="w-full text-2xs font-bold tracking-caption"
        onClick={onMaster}
      >
        MASTER
      </Button>
      <Button
        size="md"
        variant="control"
        disabled={!ready}
        aria-pressed={sync}
        aria-label={`Deck ${n} sync`}
        hint="Sync"
        shortcut={syncKey}
        className="w-full text-2xs font-bold tracking-caption"
        onClick={onSync}
      >
        SYNC
      </Button>
    </div>
  );
}
