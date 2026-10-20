/**
 * One imported play session in the main area, in play order. The header
 * renames it, sets venue and notes (saved when the field loses focus or on
 * Enter), toggles Practice (left out of play counts, D23) and deletes it
 * after a confirmation. Unmatched entries are greyed with "Locate…" to link
 * them to a library track; divider entries are hidden unless shown.
 * Double-click a matched entry to preview it.
 */
import { type KeyboardEvent, useState } from "react";

import { formatBpm, formatDuration } from "../lib/format";
import type { SessionDetail, SessionEntryRow } from "../lib/ipc";
import { useHistory } from "../store/history";
import { usePlayer } from "../store/player";
import { ConfirmDialog } from "./ConfirmDialog";
import { KeyLabel } from "./KeyLabel";
import { LocateDialog } from "./LocateDialog";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";
import { Input } from "./ui/Input";
import { ScrollArea } from "./ui/ScrollArea";
import { Switch } from "./ui/Switch";
import { Textarea } from "./ui/Textarea";
import { Tooltip } from "./ui/Tooltip";

export function SessionView() {
  const detail = useHistory((s) => s.detail);
  const error = useHistory((s) => s.error);
  const confirmDelete = useHistory((s) => s.confirmDelete);
  const locating = useHistory((s) => s.locating);
  const h = useHistory.getState;

  if (detail === null) {
    return (
      <section className="flex flex-1 items-center justify-center text-fg-muted" aria-busy>
        {error ?? "Loading…"}
      </section>
    );
  }
  const locateEntry = locating === null ? undefined : detail.entries[locating];
  return (
    <section className="flex min-h-0 flex-1 flex-col" aria-label="Play session">
      <SessionHeader key={detail.session.id} detail={detail} />
      {error !== null && (
        <div className="border-b border-border px-2.5 py-1 text-danger select-text" role="alert">
          {error}
        </div>
      )}
      <EntryList detail={detail} />
      {confirmDelete !== null && (
        <ConfirmDialog
          title="Delete session?"
          buttons={[
            { label: "Delete", primary: true, onClick: () => void h().resolveDelete(true) },
            { label: "Cancel", onClick: () => void h().resolveDelete(false) },
          ]}
        >
          “{confirmDelete.name}” and its {confirmDelete.tracks} entries are removed from the library
          and play counts are recomputed. The stick is not changed; importing it again brings the
          session back.
        </ConfirmDialog>
      )}
      {locateEntry !== undefined && <LocateDialog entry={locateEntry} />}
    </section>
  );
}

/** Saves a text field on blur or Enter, when it changed. */
function useDraft(saved: string, commit: (v: string) => void) {
  const [draft, setDraft] = useState(saved);
  return {
    props: {
      value: draft,
      onChange: (e: { target: { value: string } }) => setDraft(e.target.value),
      onBlur: () => {
        if (draft !== saved) commit(draft);
      },
    },
    reset: () => setDraft(saved),
  };
}

function SessionHeader({ detail }: { detail: SessionDetail }) {
  const s = detail.session;
  const update = useHistory((st) => st.update);
  const requestDelete = useHistory((st) => st.requestDelete);
  const name = useDraft(s.name, (v) => {
    if (v.trim() === "") name.reset();
    else void update({ name: v });
  });
  const venue = useDraft(s.venue ?? "", (v) => void update({ venue: v.trim() === "" ? null : v }));
  const notes = useDraft(detail.notes ?? "", (v) => update({ notes: v.trim() === "" ? null : v }));
  const enterSaves = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") e.currentTarget.blur();
  };
  const dividers = detail.entries.filter((e) => e.kind === "divider").length;

  return (
    <header className="flex shrink-0 flex-col gap-2 border-b border-border bg-bg-subtle px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5">
        <Input
          className="max-w-90 min-w-40 flex-1 font-semibold"
          aria-label="Session name"
          {...name.props}
          onKeyDown={enterSaves}
        />
        <span className="text-sm text-fg-muted">
          {s.playedOn ?? "Date unknown"} · {s.tracks} track{s.tracks === 1 ? "" : "s"}
          {s.unmatched > 0 && ` · ${s.unmatched} not in library`}
        </span>
        <span className="flex-1" />
        <Tooltip content="Practice sessions are kept but not counted in play count or last played">
          {/* biome-ignore lint/a11y/noLabelWithoutControl: the Switch is the control. */}
          <label className="flex items-center gap-1.5 text-sm">
            <Switch
              size="md"
              aria-label="Practice"
              checked={s.excluded}
              onCheckedChange={(on) => void update({ excluded: on })}
            />
            Practice
          </label>
        </Tooltip>
        <Button variant="danger" size="sm" onClick={requestDelete}>
          Delete…
        </Button>
      </div>
      {s.excluded && s.excludedReason === "pre_baseline" && (
        <p className="m-0 text-xs text-fg-muted">
          Imported as practice: played on or before the rekordbox baseline, so rekordbox may already
          have counted it. Turn Practice off to count it.
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <Field label="Venue" className="w-60">
          <Input size="sm" placeholder="Where you played" {...venue.props} onKeyDown={enterSaves} />
        </Field>
        <Field label="Notes" className="min-w-48 flex-1">
          <Textarea size="sm" rows={1} placeholder="Anything to remember" {...notes.props} />
        </Field>
      </div>
      <div className="flex items-center gap-2 text-xs text-fg-muted">
        <span>
          From {s.sourceName}
          {s.library === "oneLibrary" && " (OneLibrary)"}
          {detail.volumeSerial !== null && ` on stick ${detail.volumeSerial}`}, imported{" "}
          {s.importedAt.slice(0, 10)}
        </span>
        {dividers > 0 && <ShowDividers count={dividers} />}
      </div>
    </header>
  );
}

function ShowDividers({ count }: { count: number }) {
  const on = useHistory((s) => s.showDividers);
  const set = useHistory((s) => s.setShowDividers);
  return (
    <Button size="xs" variant="ghost" aria-pressed={on} onClick={() => set(!on)}>
      {on ? "Hide" : "Show"} {count} divider{count === 1 ? "" : "s"}
    </Button>
  );
}

const TH = "sticky top-0 bg-bg px-2 py-1 text-left text-xs font-normal text-fg-muted";
const TD = "h-(--row-h) px-2 whitespace-nowrap overflow-hidden text-ellipsis";

function EntryList({ detail }: { detail: SessionDetail }) {
  const showDividers = useHistory((s) => s.showDividers);
  const startLocate = useHistory((s) => s.startLocate);
  let n = 0;
  const rows = detail.entries.flatMap((e): { e: SessionEntryRow; n: number | null }[] => {
    if (e.kind === "divider") return showDividers ? [{ e, n: null }] : [];
    n += 1;
    return [{ e, n }];
  });
  return (
    <ScrollArea className="min-h-0 flex-1">
      <table className="w-full table-fixed border-collapse text-md" aria-label="Session entries">
        <colgroup>
          <col className="w-11" />
          <col />
          <col className="w-1/4" />
          <col className="w-16" />
          <col className="w-14" />
          <col className="w-16" />
          <col className="w-24" />
        </colgroup>
        <thead>
          <tr>
            <th className={`${TH} text-right`}>#</th>
            <th className={TH}>Title</th>
            <th className={TH}>Artist</th>
            <th className={`${TH} text-right`}>BPM</th>
            <th className={TH}>Key</th>
            <th className={`${TH} text-right`}>Time</th>
            <th className={TH}>
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ e, n }) =>
            e.kind === "divider" ? (
              <tr key={e.position} className="bg-surface-strong text-fg-muted" data-divider="">
                <td className={TD} />
                <td className={`${TD} text-xs font-semibold tracking-wide`} colSpan={6}>
                  {e.rawTitle ?? "Divider"}
                </td>
              </tr>
            ) : (
              <EntryRow key={e.position} entry={e} n={n ?? 0} onLocate={startLocate} />
            ),
          )}
        </tbody>
      </table>
      {rows.length === 0 && <p className="m-3 text-fg-muted">This session has no entries.</p>}
    </ScrollArea>
  );
}

function EntryRow({
  entry: e,
  n,
  onLocate,
}: {
  entry: SessionEntryRow;
  n: number;
  onLocate: (position: number) => void;
}) {
  const matched = e.trackId !== null;
  const trackId = e.trackId;
  return (
    <tr
      className="odd:bg-row-alt data-[unmatched]:text-fg-subtle"
      data-unmatched={matched ? undefined : ""}
      title={matched ? undefined : `Not in the library: ${e.rawPath ?? "no path on the stick"}`}
      onDoubleClick={() => {
        if (trackId === null) return;
        void usePlayer.getState().load({ id: trackId, title: e.title, artist: e.artist });
      }}
    >
      <td className={`${TD} text-right tabular-nums text-fg-muted`}>{n}</td>
      <td className={TD}>{matched ? e.title : e.rawTitle}</td>
      <td className={TD}>{matched ? e.artist : e.rawArtist}</td>
      <td className={`${TD} text-right tabular-nums`}>{formatBpm(e.bpmX100)}</td>
      <td className={TD}>{e.key === null ? "" : <KeyLabel value={e.key} />}</td>
      <td className={`${TD} text-right tabular-nums`}>
        {formatDuration(matched ? e.durationMs : e.rawDurationMs)}
      </td>
      <td className={`${TD} text-right`}>
        {matched ? null : (
          <Button size="xs" variant="link" onClick={() => onLocate(e.position)}>
            Locate…
          </Button>
        )}
      </td>
    </tr>
  );
}
