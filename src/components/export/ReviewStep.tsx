/**
 * Export step 1, Review (D42): one headline and three numbers, the track
 * issues with their fixes (none blocks), the play-history line or guard,
 * the settings in use with "Change…", and everything technical collapsed.
 * Nothing is written before Export. All wording comes from Rust (`DryRun`).
 */
import { useState } from "react";

import type { DryRun, HistoryGuardView, HistoryPolicy, OneLibraryRemovalView } from "../../lib/ipc";
import { useExportWizard } from "../../store/exportWizard";
import { ErrorMessage } from "../ErrorMessage";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Checkbox } from "../ui/Checkbox";
import { Chip } from "../ui/Chip";
import { Progress } from "../ui/Progress";
import { IssueCard } from "./IssueCard";
import { Facts, Files, Notices, Section } from "./parts";

/** Indent per tree level, as in the sidebar tree. */
const INDENT = 18;

export function ReviewStep() {
  const dryRun = useExportWizard((s) => s.dryRun);
  const planning = useExportWizard((s) => s.planning);
  const error = useExportWizard((s) => s.error);
  if (error !== null) {
    return (
      <div className="flex flex-col items-start gap-2">
        <ErrorMessage as="p" className="m-0" message={error} />
        {dryRun === null && useExportWizard.getState().selection !== null && (
          <Button onClick={() => void useExportWizard.getState().replan()}>Try again</Button>
        )}
      </div>
    );
  }
  if (dryRun === null) {
    return (
      <div className="flex flex-col gap-2">
        <p className="m-0">Reading the stick and checking your files…</p>
        <Progress aria-label="Planning" />
      </div>
    );
  }
  return <ReviewView dryRun={dryRun} planning={planning} />;
}

function ReviewView({ dryRun: d, planning }: { dryRun: DryRun; planning: boolean }) {
  const kept = useExportWizard((s) => s.kept);
  const w = useExportWizard.getState();
  const o = d.overview;
  const open = d.issues.filter((i) => !kept.has(i.kind));
  return (
    <div className="flex flex-col gap-3 select-text" aria-busy={planning || undefined}>
      <div className="flex items-start justify-between gap-4">
        <p
          className={d.ready ? "m-0 font-semibold" : "m-0 font-semibold text-danger"}
          role={d.ready ? "status" : "alert"}
        >
          {o.headline}
        </p>
        {open.length > 1 && (
          <Button size="sm" variant="link" disabled={planning} onClick={() => w.keep(null)}>
            Keep all as they are
          </Button>
        )}
      </div>
      {planning && <Progress aria-label="Planning again" />}

      <section className="grid grid-cols-1 gap-2 sm:grid-cols-3" aria-label="Summary">
        <Tile value={o.playlistsText} detail={`${o.tracksExported} tracks`} />
        <Tile value={o.copyText} detail={o.copyDetail} />
        <Tile
          value={o.spaceText}
          detail={o.spaceDetail}
          tone={d.space.fits ? undefined : "danger"}
        />
      </section>

      {d.problems.length > 0 && (
        <Section title="The stick can't be written yet" tone="danger">
          <Notices groups={d.problems} />
        </Section>
      )}

      {d.issues.map((issue) => (
        <IssueCard
          key={issue.kind}
          issue={issue}
          kept={kept.has(issue.kind)}
          disabled={planning}
          handlers={{
            onFix: (action) => {
              if (action.type === "findFiles") void w.runFix("findFiles", issue.trackIds);
              else if (action.type === "convert") void w.runFix("convert", action.trackIds);
              else if (action.type === "analyse") void w.runFix("analyse", issue.trackIds);
              else if (action.type === "leaveOut") void w.leaveOutUnanalysed();
            },
            onRemove: (tracks) => void w.removeFromPlaylists(tracks),
            onKeep: () => w.keep(issue.kind),
          }}
        />
      ))}

      {d.history.guard !== null && (
        <HistoryGuard guard={d.history.guard} policy={d.history.policy} disabled={planning} />
      )}
      {d.history.guard === null && (
        <p className="m-0 text-sm text-fg-muted">
          {d.history.atRisk ? (d.history.actionText ?? d.history.text) : `${d.history.title}.`}
        </p>
      )}

      {d.onelibraryRemoval !== null && (
        <OnelibraryRemoval removal={d.onelibraryRemoval} disabled={planning} />
      )}

      {d.warnings.length > 0 && (
        <Section title="Good to know" tone="warning">
          <Notices groups={d.warnings} />
          {d.convertText !== null && (
            <div className="flex items-center gap-2">
              <p className="m-0 text-sm">{d.convertText}</p>
              <Button
                size="sm"
                disabled={planning}
                onClick={() => void w.runFix("convert", d.convertible)}
              >
                Convert…
              </Button>
            </div>
          )}
        </Section>
      )}

      <section className="flex flex-wrap items-center gap-1.5" aria-label="Writing with">
        <span className="text-xs text-fg-muted">Writing with:</span>
        {d.settings.map((s) => (
          <Chip key={s}>{s}</Chip>
        ))}
        <Button
          size="sm"
          variant="link"
          disabled={planning}
          onClick={() => void w.changeSettings()}
        >
          Change…
        </Button>
      </section>

      <details>
        <summary className="text-sm">
          Technical details: files to copy and delete, paths, file system
        </summary>
        <Card className="mt-2 flex flex-col gap-2 px-4 py-3" divided={false}>
          <p className="m-0 text-xs text-fg-muted">
            {d.device.name} · {d.device.fsText} · {d.device.spaceText} · target {d.target}
          </p>
          <p className="m-0 text-xs text-fg-muted">{d.status}</p>
          <Facts facts={d.summary} />
          <Facts facts={d.space.facts} />
          <Files label="Copy" group={d.copies} />
          <Files label="Delete" group={d.deletes} />
          <Files label="Play history backed up" group={d.history.backups} />
          {d.onelibrary !== null && (
            <>
              <p className="m-0 text-sm">{d.onelibrary.text}</p>
              <p className="m-0 font-mono text-xs [overflow-wrap:anywhere]">
                Write {d.onelibrary.write.path}{" "}
                <span className="text-fg-muted">({d.onelibrary.write.sizeText})</span>
              </p>
              <Files label="Remove" group={d.onelibrary.removes} />
            </>
          )}
          <details>
            <summary className="text-sm">Playlists on the stick</summary>
            <ul className="m-0 mt-1 list-none p-0 text-xs">
              {d.playlists.map((p) => (
                <li key={p.id} style={{ paddingLeft: p.depth * INDENT }}>
                  {p.name}
                  {p.isFolder ? "/" : ""}
                  {p.detail !== "" && <span className="text-fg-muted"> · {p.detail}</span>}
                  {p.smart && (
                    <span className="text-fg-muted"> · smart, written as a fixed list</span>
                  )}
                </li>
              ))}
            </ul>
          </details>
        </Card>
      </details>
    </div>
  );
}

function Tile({
  value,
  detail,
  tone,
}: {
  value: string;
  detail: string;
  tone?: "danger" | undefined;
}) {
  return (
    <Card className="flex flex-col px-3 py-2" divided={false}>
      <span
        className={tone === "danger" ? "font-semibold text-danger" : "font-semibold tabular-nums"}
      >
        {value}
      </span>
      {detail !== "" && <span className="text-xs text-fg-muted tabular-nums">{detail}</span>}
    </Card>
  );
}

/** The stick's `exportLibrary.db` is deleted: what goes, for which players, and the confirmation. */
function OnelibraryRemoval({
  removal,
  disabled,
}: {
  removal: OneLibraryRemovalView;
  disabled: boolean;
}) {
  return (
    <Section title={removal.title} tone={removal.confirmed ? "warning" : "danger"}>
      <p className="m-0 text-sm">{removal.text}</p>
      <Files label="Delete" group={removal.files} />
      <label className="flex items-start gap-2 text-sm">
        <Checkbox
          className="mt-0.5"
          checked={removal.confirmed}
          disabled={removal.confirmed || disabled}
          onChange={(e) => {
            if (e.target.checked) void useExportWizard.getState().confirmOnelibraryRemoval();
          }}
        />
        <span>{removal.confirmText}</span>
      </label>
    </Section>
  );
}

const GUARD_CHOICES: Record<HistoryPolicy, string> = {
  import: "Import now",
  backUp: "Back up and continue",
  discard: "Discard",
};

/** The export guard (D23): sessions the library has not imported. Discard is behind "More". */
function HistoryGuard({
  guard: g,
  policy,
  disabled,
}: {
  guard: HistoryGuardView;
  policy: HistoryPolicy;
  disabled: boolean;
}) {
  const [more, setMore] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [understood, setUnderstood] = useState(false);
  const choose = (p: HistoryPolicy) => {
    setConfirming(false);
    setUnderstood(false);
    void useExportWizard.getState().chooseHistory(p);
  };
  if (g.chosen) {
    return (
      <p className="m-0 text-sm text-fg-muted" role="status">
        ✓ {g.title}: {GUARD_CHOICES[policy]}.
      </p>
    );
  }
  return (
    <Section title={g.title} tone="danger">
      <p className="m-0 text-sm">{g.text}</p>
      {g.sessions.length > 0 && (
        <ul className="m-0 list-disc pl-5 text-xs">
          {g.sessions.map((s) => (
            <li key={s}>{s}</li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {g.canImport && (
          <Button size="sm" variant="primary" disabled={disabled} onClick={() => choose("import")}>
            {GUARD_CHOICES.import}
          </Button>
        )}
        <Button
          size="sm"
          variant={g.canImport ? "secondary" : "primary"}
          disabled={disabled}
          onClick={() => choose("backUp")}
        >
          {GUARD_CHOICES.backUp}
        </Button>
        {!more ? (
          <Button size="sm" variant="ghost" disabled={disabled} onClick={() => setMore(true)}>
            More
          </Button>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            disabled={disabled || confirming}
            onClick={() => setConfirming(true)}
          >
            {GUARD_CHOICES.discard}…
          </Button>
        )}
      </div>
      {confirming && (
        <div className="flex flex-col items-start gap-2">
          <label className="flex items-start gap-2 text-sm text-warning">
            <Checkbox
              className="mt-0.5"
              checked={understood}
              onChange={(e) => setUnderstood(e.target.checked)}
            />
            <span>
              I understand these sessions are erased from the stick without a backup and can't be
              imported later.
            </span>
          </label>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="danger"
              disabled={!understood || disabled}
              onClick={() => choose("discard")}
            >
              Discard and continue
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </Section>
  );
}
