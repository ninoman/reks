/**
 * MASTER (lit while this deck is the sync master; a click takes master) and
 * SYNC. Both need a grid. The player deck alone has neither. `compact`
 * places them in the parent's two-column grid (SYNC under PLAY, MASTER below).
 */
import { cn } from "../../lib/cn";
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
  compact?: boolean;
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
  compact = false,
}: DeckSyncButtonsProps) {
  const n = deck + 1;
  const size = compact ? "xs" : "md";
  const buttons = (
    <>
      <Button
        size={size}
        variant="control"
        disabled={!ready}
        aria-pressed={master}
        aria-label={`Deck ${n} master`}
        hint={master ? "Master" : "Take master"}
        shortcut={masterKey}
        className={cn(
          "w-full text-2xs font-bold tracking-caption",
          compact && "col-span-2 row-start-3",
        )}
        onClick={onMaster}
      >
        MASTER
      </Button>
      <Button
        size={size}
        variant="control"
        disabled={!ready}
        aria-pressed={sync}
        aria-label={`Deck ${n} sync`}
        hint="Sync"
        shortcut={syncKey}
        className={cn(
          "w-full text-2xs font-bold tracking-caption",
          compact && "col-start-2 row-start-2 px-0",
        )}
        onClick={onSync}
      >
        SYNC
      </Button>
    </>
  );
  if (compact) return buttons;
  return (
    <div className="ml-auto flex w-(--deck-sync-w) shrink-0 flex-col justify-end gap-2">
      {buttons}
    </div>
  );
}
