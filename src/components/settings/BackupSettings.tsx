/**
 * Settings → Library → Backups (D36): the library's automatic snapshots, Back
 * up now, Show in folder, how many recent ones to keep, and Restore. A
 * restore snapshots the current library first, then restarts the app.
 */
import { useEffect, useState } from "react";
import { reasonLabel, restoreBlock } from "../../lib/backups";
import { cn } from "../../lib/cn";
import { formatBytes, formatDateTime } from "../../lib/format";
import type { BackupInfo } from "../../lib/ipc";
import { useBackups } from "../../store/backups";
import { useSettings } from "../../store/settings";
import { ErrorMessage } from "../ErrorMessage";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { InlineNumber } from "../ui/InlineNumber";
import { settingItemClass } from "./SettingItem";

const hint = "mt-0.5 mb-0 text-xs text-fg-muted";
const row = "mt-3 flex items-center gap-2";

export function BackupSettings({ keep }: { keep: number }) {
  const { backups, status, busy, error } = useBackups();
  const [target, setTarget] = useState<BackupInfo | null>(null);

  useEffect(() => {
    void useBackups.getState().load();
  }, []);

  const failure = status?.lastFailure ?? null;
  const { backUpNow, showFolder, restore } = useBackups.getState();

  return (
    <div className={settingItemClass}>
      <h5 className="m-0 text-sm font-semibold">Library backups</h5>
      <p className={hint}>
        Cues, grids, tags and playlists live only in the library. reks saves a copy at start-up
        (once a day), before updates and before big imports, and keeps the newest {keep} plus one a
        week for 8 weeks.
      </p>
      {failure !== null && (
        <ErrorMessage
          as="p"
          className="mt-2.5 mb-0"
          message={`The last backup failed: ${failure.message}`}
        />
      )}
      {error !== null && <ErrorMessage as="p" className="mt-2.5 mb-0" message={error} />}

      <div className={row}>
        <Button disabled={busy !== null} onClick={() => void backUpNow()}>
          {busy === "backup" ? "Backing up…" : "Back up now"}
        </Button>
        <Button disabled={status === null || status.dir === null} onClick={() => void showFolder()}>
          Show in folder
        </Button>
        <span className="ml-auto flex items-center gap-1.5 text-xs whitespace-nowrap text-fg-muted">
          Keep newest
          <InlineNumber
            value={keep}
            format={String}
            aria-label="Recent backups kept"
            title="Double-click to change"
            onCommit={(n) => void useSettings.getState().update({ backupKeep: Math.round(n) })}
          />
        </span>
      </div>

      <BackupList backups={backups} busy={busy} onRestore={setTarget} />

      {target !== null && (
        <div className="mt-3 rounded-md border border-border bg-surface px-3 py-2.5">
          <p className="m-0">
            Restore the backup from {formatDateTime(target.takenAt)}? Your current library is saved
            first, then reks restarts.
          </p>
          <div className={row}>
            <Button
              variant="primary"
              disabled={busy !== null}
              onClick={() => void restore(target.name)}
            >
              {busy === "restore" ? "Restoring…" : "Restore and restart"}
            </Button>
            <Button disabled={busy !== null} onClick={() => setTarget(null)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function BackupList({
  backups,
  busy,
  onRestore,
}: {
  backups: BackupInfo[] | null;
  busy: "backup" | "restore" | null;
  onRestore: (b: BackupInfo) => void;
}) {
  if (backups === null) return <p className={cn(hint, "mt-3")}>Loading backups…</p>;
  if (backups.length === 0) {
    return <p className={cn(hint, "mt-3")}>No backups yet. Back up now to make the first one.</p>;
  }
  return (
    <Card
      variant="surface"
      className="mt-3 rounded-md [&_li]:grid [&_li]:grid-cols-[1fr_auto] [&_li]:items-center [&_li]:gap-3 [&_li]:py-1.5 [&_li]:pr-1.5 [&_li]:pl-2.5"
      asChild
    >
      <ul aria-label="Backups" className="m-0 list-none p-0">
        {backups.map((b) => {
          const block = restoreBlock(b);
          return (
            <li key={b.name}>
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-sm">
                  <span>{formatDateTime(b.takenAt)}</span>
                  <span className="text-fg-muted">{reasonLabel(b.reason)}</span>
                  {block !== null && (
                    <Badge variant="danger" size="md" title={b.problem ?? block}>
                      {b.problem !== null ? "Damaged" : "Newer reks"}
                    </Badge>
                  )}
                </div>
                <div className="text-xs text-fg-muted">
                  {b.contents === null
                    ? formatBytes(b.sizeBytes)
                    : `${b.contents.tracks} tracks, ${b.contents.playlists} playlists, ${formatBytes(b.sizeBytes)}`}
                </div>
              </div>
              <Button
                variant="quiet"
                size="sm"
                aria-label={`Restore the backup from ${formatDateTime(b.takenAt)}`}
                title={block ?? undefined}
                disabled={block !== null || busy !== null}
                onClick={() => onRestore(b)}
              >
                Restore…
              </Button>
            </li>
          );
        })}
      </ul>
    </Card>
  );
}
