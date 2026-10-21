/**
 * The rekordbox XML import wizard (phase 1 task 12): review the counts and
 * map a moved music folder before anything is written, follow the import,
 * then read the report. All counting and matching happens in Rust; this only
 * shows `ImportPreview` / `ImportReport`.
 */
import { open } from "@tauri-apps/plugin-dialog";

import type { ImportPreview, ImportReport, Unmatched, UnmatchedReason } from "../lib/ipc";
import { useImportWizard } from "../store/importWizard";
import { Button } from "./ui/Button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { Progress } from "./ui/Progress";

const counts =
  "m-0 mb-3 grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 [&_dt]:text-fg-muted [&_dd]:m-0 [&_dd]:text-right [&_dd]:tabular-nums";
const pathList = "my-1 mb-2 list-disc pl-4.5 font-mono text-xs [overflow-wrap:anywhere]";

function n(v: number): string {
  return v.toLocaleString();
}

function plural(v: number, word: string): string {
  return `${n(v)} ${word}${v === 1 ? "" : "s"}`;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

async function pickFolder(from: string): Promise<string | null> {
  const to = await open({ directory: true, title: `Where is "${from}" now?` });
  return typeof to === "string" ? to : null;
}

export function ImportWizard() {
  const step = useImportWizard((s) => s.step);
  const path = useImportWizard((s) => s.path);
  if (step === "closed" || path === null) return null;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) useImportWizard.getState().close();
      }}
    >
      <DialogContent size="lg" className="select-text">
        <DialogHeader>
          <DialogTitle>Import rekordbox XML</DialogTitle>
          <DialogDescription title={path}>{fileName(path)}</DialogDescription>
        </DialogHeader>
        {step === "review" && <Review />}
        {step === "importing" && <Importing />}
        {step === "report" && <Report />}
      </DialogContent>
    </Dialog>
  );
}

function Review() {
  const preview = useImportWizard((s) => s.preview);
  const previewing = useImportWizard((s) => s.previewing);
  const remap = useImportWizard((s) => s.remap);
  const error = useImportWizard((s) => s.error);
  const { start, close, addRemap, removeRemap } = useImportWizard.getState();

  async function mapFolder(from: string) {
    const to = await pickFolder(from);
    if (to !== null) await addRemap(from, to);
  }

  return (
    <>
      <DialogBody>
        {error !== null && (
          <p className="text-danger" role="alert">
            {error}
          </p>
        )}
        {preview === null ? (
          previewing && <p className="text-fg-muted">Reading the collection…</p>
        ) : (
          <PreviewCounts preview={preview} />
        )}
        {preview !== null && preview.filesNotFound > 0 && (
          <div>
            <p className="text-warning">
              {plural(preview.filesNotFound, "file")} not found
              {preview.missingRoot !== null ? ` in ${preview.missingRoot}` : ""}. They are imported
              as missing (cues, grids and playlists kept) unless you map their folder to where it is
              now.
            </p>
            <ul className={pathList}>
              {preview.notFoundSample.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
            {preview.missingRoot !== null && (
              <Button
                size="sm"
                disabled={previewing}
                onClick={() => void mapFolder(preview.missingRoot as string)}
              >
                Map folder…
              </Button>
            )}
          </div>
        )}
        {remap.length > 0 && (
          <div>
            <div className="text-fg-muted">Folder mappings</div>
            <ul className={pathList}>
              {remap.map((r, i) => (
                <li key={`${r.from}\u0000${r.to}`}>
                  <span>{r.from}</span> → <span>{r.to}</span>{" "}
                  <Button
                    variant="link"
                    size="sm"
                    aria-label={`Remove mapping of ${r.from}`}
                    onClick={() => void removeRemap(i)}
                  >
                    Remove
                  </Button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </DialogBody>
      <DialogFooter>
        <Button
          variant="primary"
          disabled={preview === null || previewing || preview.total === 0}
          onClick={() => void start()}
        >
          Import
        </Button>
        <Button onClick={close}>Cancel</Button>
      </DialogFooter>
    </>
  );
}

function PreviewCounts({ preview: p }: { preview: ImportPreview }) {
  const notStored = p.noLocation + p.notAFile + p.invalidLocation;
  return (
    <dl className={counts}>
      <dt>Tracks</dt>
      <dd>{n(p.total)}</dd>
      <dt>Files found</dt>
      <dd>{n(p.filesFound)}</dd>
      <dt>Files not found</dt>
      <dd className={p.filesNotFound > 0 ? "text-warning" : undefined}>{n(p.filesNotFound)}</dd>
      {notStored > 0 && (
        <>
          <dt>Not stored (no local file)</dt>
          <dd>{n(notStored)}</dd>
        </>
      )}
      <dt>Already in library</dt>
      <dd>{n(p.inLibrary)}</dd>
      <dt>With beatgrid / cues</dt>
      <dd>
        {n(p.withGrid)} / {n(p.withCues)}
      </dd>
      <dt>Playlists / folders</dt>
      <dd>
        {n(p.playlists)} / {n(p.folders)}
      </dd>
    </dl>
  );
}

function Importing() {
  const progress = useImportWizard((s) => s.progress);
  const unmatched = useImportWizard((s) => s.unmatchedSoFar);
  const stopping = useImportWizard((s) => s.stopping);
  const label =
    progress === null
      ? "Reading the collection…"
      : `${progress.stage === "files" ? "Reading files" : "Saving"} ${n(progress.done)} / ${n(progress.total)}`;
  const saving = progress?.stage === "database";
  return (
    <>
      <DialogBody>
        <p>{stopping ? "Stopping…" : label}</p>
        <Progress
          max={progress?.total ?? 1}
          value={progress?.done ?? undefined}
          aria-label="Import progress"
        />
        {unmatched > 0 && (
          <p className="text-warning">{plural(unmatched, "track")} without a file</p>
        )}
      </DialogBody>
      <DialogFooter>
        <Button
          disabled={saving || stopping}
          title={saving ? "Saving runs as one step and can't be stopped" : undefined}
          onClick={() => void useImportWizard.getState().cancel()}
        >
          Stop
        </Button>
      </DialogFooter>
    </>
  );
}

const REASONS: { kind: UnmatchedReason["kind"]; title: string }[] = [
  { kind: "fileNotFound", title: "File not found (stored as missing)" },
  { kind: "unreadable", title: "File not readable as audio" },
  { kind: "duplicateOf", title: "Same file as an earlier track (skipped)" },
  { kind: "notAFile", title: "Not a local file (skipped)" },
  { kind: "noLocation", title: "No location (skipped)" },
  { kind: "invalidLocation", title: "Location can't be read (skipped)" },
];

function reasonDetail(u: Unmatched): string {
  const r = u.reason;
  switch (r.kind) {
    case "fileNotFound":
      return r.path;
    case "unreadable":
      return `${r.path}: ${r.message}`;
    case "duplicateOf":
      return `same file as TrackID ${r.trackId}`;
    case "invalidLocation":
      return `${u.location ?? ""}: ${r.message}`;
    default:
      return u.location ?? "";
  }
}

function Report() {
  const report = useImportWizard((s) => s.report);
  const { close, remapAndReview } = useImportWizard.getState();
  if (report === null) return null;

  async function mapAndReview(from: string) {
    const to = await pickFolder(from);
    if (to !== null) await remapAndReview(from, to);
  }

  const canRemap = !report.cancelled && report.missing > 0 && report.missingRoot !== null;
  return (
    <>
      <DialogBody>
        {report.cancelled ? (
          <p>Cancelled: nothing was imported.</p>
        ) : (
          <ReportBody report={report} />
        )}
      </DialogBody>
      <DialogFooter>
        {canRemap && (
          <Button onClick={() => void mapAndReview(report.missingRoot as string)}>
            Map folder and re-import…
          </Button>
        )}
        <Button variant="primary" onClick={close}>
          Done
        </Button>
      </DialogFooter>
    </>
  );
}

function ReportBody({ report: r }: { report: ImportReport }) {
  const groups = REASONS.map((g) => ({
    ...g,
    rows: r.unmatched.filter((u) => u.reason.kind === g.kind),
  })).filter((g) => g.rows.length > 0);
  return (
    <>
      <dl className={counts}>
        <dt>Tracks</dt>
        <dd>{n(r.total)}</dd>
        <dt>New</dt>
        <dd>{n(r.added)}</dd>
        <dt>Updated (by path / earlier location / artist+title+size)</dt>
        <dd>
          {n(r.matchedByPath)} / {n(r.matchedByLocation)} / {n(r.matchedByFallback)}
        </dd>
        <dt>Missing files</dt>
        <dd className={r.missing > 0 ? "text-warning" : undefined}>{n(r.missing)}</dd>
        <dt>Beatgrid markers / cues</dt>
        <dd>
          {n(r.beatgridRows)} / {n(r.cueRows)}
        </dd>
        <dt>Playlists created / matched</dt>
        <dd>
          {n(r.playlistsCreated)} / {n(r.playlistsMatched)}
        </dd>
        <dt>Playlist entries (skipped)</dt>
        <dd>
          {n(r.playlistEntries)} ({n(r.playlistEntriesSkipped)})
        </dd>
        <dt>Tags created / assigned</dt>
        <dd>
          {n(r.tagsCreated)} / {n(r.tagsAssigned)}
        </dd>
        <dt>Time</dt>
        <dd>{(r.elapsedMs / 1000).toFixed(1)} s</dd>
      </dl>
      {r.missing > 0 && r.missingRoot !== null && (
        <p className="text-warning">Missing files are under {r.missingRoot}.</p>
      )}
      {groups.map((g) => (
        <details key={g.kind} className="mt-2">
          <summary>
            {g.title}: {n(g.rows.length)}
          </summary>
          <table className="w-full border-collapse text-xs [&_td]:py-0.5 [&_td]:pr-1.5 [&_td]:align-top">
            <tbody>
              {g.rows.map((u) => (
                <tr key={u.trackId}>
                  <td>{u.artist ?? ""}</td>
                  <td>{u.name ?? ""}</td>
                  <td className="text-fg-muted [overflow-wrap:anywhere]">{reasonDetail(u)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      ))}
      {r.warnings.length > 0 && (
        <details className="mt-2">
          <summary>Warnings: {n(r.warnings.length)}</summary>
          <ul className={pathList}>
            {r.warnings.map((w, i) => (
              // Warnings have no id; their order is stable for one report.
              // biome-ignore lint/suspicious/noArrayIndexKey: see above
              <li key={i}>
                {w.trackId !== null ? `TrackID ${w.trackId}: ` : ""}
                {w.message}
              </li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
}
