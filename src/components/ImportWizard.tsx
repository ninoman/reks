/**
 * The rekordbox XML import wizard (phase 1 task 12): review the counts and
 * map a moved music folder before anything is written, follow the import,
 * then read the report. All counting and matching happens in Rust; this only
 * shows `ImportPreview` / `ImportReport`.
 */
import { open } from "@tauri-apps/plugin-dialog";

import type { ImportPreview, ImportReport, Unmatched, UnmatchedReason } from "../lib/ipc";
import { useImportWizard } from "../store/importWizard";

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
    <div className="dialog-backdrop">
      <div
        className="dialog wizard"
        role="dialog"
        aria-modal="true"
        aria-label="Import rekordbox XML"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            useImportWizard.getState().close();
          }
        }}
      >
        <h3>Import rekordbox XML</h3>
        <div className="wizard-file" title={path}>
          {fileName(path)}
        </div>
        {step === "review" && <Review />}
        {step === "importing" && <Importing />}
        {step === "report" && <Report />}
      </div>
    </div>
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
      <div className="dialog-body">
        {error !== null && (
          <p className="wizard-error" role="alert">
            {error}
          </p>
        )}
        {preview === null ? (
          previewing && <p className="muted">Reading the collection…</p>
        ) : (
          <PreviewCounts preview={preview} />
        )}
        {preview !== null && preview.filesNotFound > 0 && (
          <div className="wizard-missing">
            <p className="warn">
              {plural(preview.filesNotFound, "file")} not found
              {preview.missingRoot !== null ? ` in ${preview.missingRoot}` : ""}. They are imported
              as missing (cues, grids and playlists kept) unless you map their folder to where it is
              now.
            </p>
            <ul className="wizard-paths">
              {preview.notFoundSample.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
            {preview.missingRoot !== null && (
              <button
                type="button"
                disabled={previewing}
                onClick={() => void mapFolder(preview.missingRoot as string)}
              >
                Map folder…
              </button>
            )}
          </div>
        )}
        {remap.length > 0 && (
          <div className="wizard-remaps">
            <div className="muted">Folder mappings</div>
            <ul>
              {remap.map((r, i) => (
                <li key={`${r.from}\u0000${r.to}`}>
                  <span>{r.from}</span> → <span>{r.to}</span>{" "}
                  <button
                    type="button"
                    className="link"
                    aria-label={`Remove mapping of ${r.from}`}
                    onClick={() => void removeRemap(i)}
                  >
                    Remove
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
      <div className="dialog-buttons">
        <button
          type="button"
          className="primary"
          disabled={preview === null || previewing || preview.total === 0}
          onClick={() => void start()}
        >
          Import
        </button>
        <button type="button" onClick={close}>
          Cancel
        </button>
      </div>
    </>
  );
}

function PreviewCounts({ preview: p }: { preview: ImportPreview }) {
  const notStored = p.noLocation + p.notAFile + p.invalidLocation;
  return (
    <dl className="wizard-counts">
      <dt>Tracks</dt>
      <dd>{n(p.total)}</dd>
      <dt>Files found</dt>
      <dd>{n(p.filesFound)}</dd>
      <dt>Files not found</dt>
      <dd className={p.filesNotFound > 0 ? "warn" : undefined}>{n(p.filesNotFound)}</dd>
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
      <div className="dialog-body">
        <p>{stopping ? "Stopping…" : label}</p>
        <progress
          max={progress?.total ?? 1}
          value={progress?.done ?? undefined}
          aria-label="Import progress"
        />
        {unmatched > 0 && <p className="warn">{plural(unmatched, "track")} without a file</p>}
      </div>
      <div className="dialog-buttons">
        <button
          type="button"
          disabled={saving || stopping}
          title={saving ? "Saving runs as one step and can't be stopped" : undefined}
          onClick={() => void useImportWizard.getState().cancel()}
        >
          Stop
        </button>
      </div>
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
      <div className="dialog-body">
        {report.cancelled ? (
          <p>Cancelled: nothing was imported.</p>
        ) : (
          <ReportBody report={report} />
        )}
      </div>
      <div className="dialog-buttons">
        {canRemap && (
          <button type="button" onClick={() => void mapAndReview(report.missingRoot as string)}>
            Map folder and re-import…
          </button>
        )}
        <button type="button" className="primary" onClick={close}>
          Done
        </button>
      </div>
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
      <dl className="wizard-counts">
        <dt>Tracks</dt>
        <dd>{n(r.total)}</dd>
        <dt>New</dt>
        <dd>{n(r.added)}</dd>
        <dt>Updated (by path / earlier location / artist+title+size)</dt>
        <dd>
          {n(r.matchedByPath)} / {n(r.matchedByLocation)} / {n(r.matchedByFallback)}
        </dd>
        <dt>Missing files</dt>
        <dd className={r.missing > 0 ? "warn" : undefined}>{n(r.missing)}</dd>
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
        <dt>Time</dt>
        <dd>{(r.elapsedMs / 1000).toFixed(1)} s</dd>
      </dl>
      {r.missing > 0 && r.missingRoot !== null && (
        <p className="warn">Missing files are under {r.missingRoot}.</p>
      )}
      {groups.map((g) => (
        <details key={g.kind} className="wizard-group">
          <summary>
            {g.title}: {n(g.rows.length)}
          </summary>
          <table>
            <tbody>
              {g.rows.map((u) => (
                <tr key={u.trackId}>
                  <td>{u.artist ?? ""}</td>
                  <td>{u.name ?? ""}</td>
                  <td className="path">{reasonDetail(u)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      ))}
      {r.warnings.length > 0 && (
        <details className="wizard-group">
          <summary>Warnings: {n(r.warnings.length)}</summary>
          <ul>
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
