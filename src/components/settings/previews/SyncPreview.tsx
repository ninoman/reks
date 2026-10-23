/**
 * Two bars of beats, the master's over the synced deck's. Bar sync lines up
 * the downbeats too; beat sync only the beats, so the deck's bar can start a
 * beat off.
 */
import { cn } from "../../../lib/cn";
import type { SyncAlign } from "../../../lib/ipc";

const BEATS = 8;

function Beats({ downbeatAt }: { downbeatAt: number }) {
  return (
    <span className="flex w-full items-center justify-between">
      {Array.from({ length: BEATS }, (_, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: the beat's place is its identity.
          key={i}
          className={cn(
            "w-0.5 rounded-full",
            i % 4 === downbeatAt ? "h-3.5 bg-grid-downbeat" : "h-2 bg-grid-beat",
          )}
        />
      ))}
    </span>
  );
}

export function SyncPreview({ align }: { align: SyncAlign }) {
  return (
    <span className="flex w-full flex-col gap-1.5 px-3">
      <Beats downbeatAt={0} />
      <Beats downbeatAt={align === "bar" ? 0 : 1} />
    </span>
  );
}
