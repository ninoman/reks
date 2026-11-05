/**
 * One track issue in the export review (D42): what happens to the tracks,
 * one row each, and the fixes Rust offers. Nothing here blocks the export;
 * "Keep as it is" collapses the card. Rows the user can remove from the
 * stick's playlists have a checkbox (smart playlists have none: their rule
 * would bring the track back). Removing asks first.
 */
import { useState } from "react";

import type { IssueAction, IssueTrack, ReviewIssue } from "../../lib/ipc";
import { Button } from "../ui/Button";
import { Checkbox } from "../ui/Checkbox";

/** Rows shown before "Show all". */
const FIRST_ROWS = 3;

export interface IssueHandlers {
  onFix(action: IssueAction, issue: ReviewIssue): void;
  onRemove(tracks: IssueTrack[]): void;
  onKeep(): void;
}

const LABELS: Record<IssueAction["type"], string> = {
  findFiles: "Find files…",
  convert: "Convert…",
  analyse: "Analyse now",
  leaveOut: "Leave them out",
  removeFromPlaylists: "Remove from playlists…",
};

export function IssueCard({
  issue,
  kept,
  disabled,
  handlers,
}: {
  issue: ReviewIssue;
  kept: boolean;
  disabled: boolean;
  /** Without handlers the card only reports (the done step). */
  handlers?: IssueHandlers;
}) {
  const removable = issue.tracks.filter((t) => t.playlists.length > 0);
  const [ticked, setTicked] = useState<ReadonlySet<number>>(
    () => new Set(removable.map((t) => t.trackId)),
  );
  const [confirming, setConfirming] = useState(false);
  const [showAll, setShowAll] = useState(false);

  if (kept) {
    return (
      <p className="m-0 text-sm text-fg-muted" role="status">
        ✓ {issue.keptText}
      </p>
    );
  }

  const chosen = removable.filter((t) => ticked.has(t.trackId));
  const playlists = new Set(chosen.flatMap((t) => t.playlists)).size;
  const rows = showAll ? issue.tracks : issue.tracks.slice(0, FIRST_ROWS);
  const more = issue.trackIds.length - rows.length;
  const canTick = handlers !== undefined && issue.kind !== "approximateAnalysis";
  const toggle = (id: number) => {
    const next = new Set(ticked);
    if (!next.delete(id)) next.add(id);
    setTicked(next);
  };

  return (
    <section
      aria-label={issue.title}
      className="flex flex-col gap-2 rounded-md border border-warning bg-surface px-3 py-2.5"
    >
      <h3 className="m-0 text-sm font-semibold text-warning">{issue.title}</h3>
      <p className="m-0 text-sm">{issue.detail}</p>
      <ul className="m-0 flex list-none flex-col gap-1 p-0" aria-label={`${issue.title}: tracks`}>
        {rows.map((t) => (
          <li key={t.trackId} className="flex items-start gap-2 text-sm">
            {canTick && t.playlists.length > 0 ? (
              <Checkbox
                className="mt-0.5"
                aria-label={`Remove ${t.label} from this stick's playlists`}
                checked={ticked.has(t.trackId)}
                onChange={() => toggle(t.trackId)}
              />
            ) : (
              canTick && <span className="w-3.5 shrink-0" aria-hidden="true" />
            )}
            <span className="min-w-0">
              <span className="block truncate">{t.label}</span>
              {(t.inText !== "" || t.detail !== "") && (
                <span className="block text-xs text-fg-muted [overflow-wrap:anywhere]">
                  {[t.inText, t.detail].filter((x) => x !== "").join(" · ")}
                </span>
              )}
            </span>
          </li>
        ))}
      </ul>
      {more > 0 &&
        (showAll || issue.tracks.length <= FIRST_ROWS ? (
          <p className="m-0 text-xs text-fg-muted">…and {more} more</p>
        ) : (
          <Button size="sm" variant="link" className="self-start" onClick={() => setShowAll(true)}>
            …and {more} more
          </Button>
        ))}
      {handlers !== undefined && !confirming && (
        <div className="flex flex-wrap items-center gap-2">
          {issue.actions.map((a) => (
            <Button
              key={a.type}
              size="sm"
              disabled={disabled || (a.type === "removeFromPlaylists" && chosen.length === 0)}
              onClick={() =>
                a.type === "removeFromPlaylists" ? setConfirming(true) : handlers.onFix(a, issue)
              }
            >
              {LABELS[a.type]}
            </Button>
          ))}
          <Button size="sm" variant="ghost" disabled={disabled} onClick={handlers.onKeep}>
            Keep as it is
          </Button>
        </div>
      )}
      {handlers !== undefined && confirming && (
        <fieldset className="m-0 flex min-w-0 flex-col items-start gap-2 border-0 p-0">
          <p className="m-0 text-sm">
            Remove {chosen.length} {chosen.length === 1 ? "track" : "tracks"} from {playlists}{" "}
            {playlists === 1 ? "playlist" : "playlists"}? The tracks stay in your library and in
            your other playlists.
          </p>
          <div className="flex gap-2">
            <Button size="sm" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              size="sm"
              variant="danger"
              disabled={disabled}
              onClick={() => {
                setConfirming(false);
                handlers.onRemove(chosen);
              }}
            >
              Remove
            </Button>
          </div>
        </fieldset>
      )}
    </section>
  );
}
