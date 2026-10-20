/**
 * "Played in" on the track info panel: play count and last played (derived,
 * D23) and the imported sessions the track was played in, newest first.
 * Practice sessions are listed but marked: they don't count. Click one to
 * open it.
 */
import { useEffect, useState } from "react";

import { historyTrackSessions, type SessionRef } from "../lib/ipc";
import { useHistory } from "../store/history";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";

export function PlayedIn({
  trackId,
  playCount,
  lastPlayed,
}: {
  trackId: number;
  playCount: number;
  lastPlayed: string | null;
}) {
  const version = useHistory((s) => s.version);
  const [sessions, setSessions] = useState<SessionRef[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` refetches after history edits.
  useEffect(() => {
    let live = true;
    historyTrackSessions(trackId).then(
      (s) => live && setSessions(s),
      (e: unknown) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [trackId, version]);

  return (
    <section aria-label="Played in">
      <h3 className="mt-0 mb-1 text-sm font-semibold">Played in</h3>
      <dl className="m-0 mb-1 grid grid-cols-[auto_1fr] gap-x-2.5 gap-y-0.5 [&_dd]:m-0 [&_dt]:text-fg-muted">
        <dt>Plays</dt>
        <dd className="tabular-nums">{playCount}</dd>
        <dt>Last played</dt>
        <dd>{lastPlayed?.slice(0, 10) ?? "Never"}</dd>
      </dl>
      {error !== null ? (
        <div className="text-danger select-text" role="alert">
          {error}
        </div>
      ) : sessions === null ? null : sessions.length === 0 ? (
        <p className="m-0 text-fg-muted">No imported sessions.</p>
      ) : (
        <ul className="m-0 flex flex-col p-0">
          {sessions.map((s) => (
            <li key={s.id} className="flex items-center gap-1.5">
              <Button
                size="xs"
                variant="link"
                className="min-w-0 justify-start truncate px-0"
                onClick={() => void useHistory.getState().open(s.id)}
              >
                {s.name}
              </Button>
              <span className="text-xs whitespace-nowrap text-fg-muted">
                {s.playedOn ?? "date unknown"}
                {s.venue !== null && ` · ${s.venue}`}
                {s.plays > 1 && ` · ×${s.plays}`}
              </span>
              {s.excluded && (
                <Badge variant="muted" title="Practice: not counted">
                  Practice
                </Badge>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
