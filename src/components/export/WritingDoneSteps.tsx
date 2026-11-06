/**
 * Export steps 2 and 3: Writing (progress from `export_apply`) and Done
 * (the report). Left-out tracks are repeated in the report, so nothing is
 * skipped silently. All wording comes from Rust.
 */
import { cn } from "../../lib/cn";
import type { ExportOutcome, ExportReportView } from "../../lib/ipc";
import { useExportWizard } from "../../store/exportWizard";
import { ErrorMessage } from "../ErrorMessage";
import { Progress } from "../ui/Progress";
import { IssueCard } from "./IssueCard";
import { Facts, list, Notices, Section } from "./parts";

export function WritingStep() {
  const progress = useExportWizard((s) => s.progress);
  const cancelling = useExportWizard((s) => s.cancelling);
  const counted = progress !== null && progress.total > 0;
  return (
    <div className="flex flex-col gap-2">
      <p className="m-0 font-semibold" role="status">
        {progress?.title ?? "Starting the export…"}
      </p>
      <Progress
        aria-label="Export progress"
        {...(counted ? { max: progress.total, value: progress.done } : {})}
      />
      {progress?.detail != null && (
        <p className="m-0 truncate text-xs text-fg-muted tabular-nums">{progress.detail}</p>
      )}
      <p className="m-0 text-xs text-fg-muted">
        Backs up play history · copies music · writes the library · checks the stick
      </p>
      {cancelling && progress?.cancellable !== false && (
        <p className="m-0 text-sm text-fg-muted">Cancelling…</p>
      )}
      {progress !== null && progress.warnings.length > 0 && (
        <ul className={cn(list, "text-warning")} aria-label="Warnings">
          {progress.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

const OUTCOME_TONE: Record<ExportOutcome, string> = {
  success: "text-success",
  problems: "text-warning",
  cancelled: "text-fg",
  failed: "text-danger",
};

export function DoneStep() {
  const report = useExportWizard((s) => s.report);
  const error = useExportWizard((s) => s.error);
  const eject = useExportWizard((s) => s.eject);
  return (
    <div className="flex flex-col gap-3 select-text">
      {error !== null && <ErrorMessage as="p" className="m-0" message={error} />}
      {report !== null && <ReportView report={report} />}
      {eject.message !== null && (
        <p
          className={cn("m-0 text-sm", eject.state === "failed" ? "text-danger" : "text-success")}
          role={eject.state === "failed" ? "alert" : "status"}
        >
          {eject.message}
        </p>
      )}
    </div>
  );
}

function ReportView({ report: r }: { report: ExportReportView }) {
  const bad = r.outcome === "failed" || r.outcome === "problems";
  return (
    <>
      <div className="flex flex-col gap-1">
        <p
          className={cn("m-0 font-semibold", OUTCOME_TONE[r.outcome])}
          role={bad ? "alert" : "status"}
        >
          {r.title}
        </p>
        <p className="m-0 text-sm">{r.status}</p>
      </div>

      {r.problems.length > 0 && (
        <Section title="Problems" tone="danger">
          <Notices groups={r.problems} />
        </Section>
      )}

      {r.leftOut.map((issue) => (
        <IssueCard key={issue.kind} issue={issue} kept={false} disabled />
      ))}

      {r.warnings.length > 0 && (
        <Section title="Warnings" tone="warning">
          <Notices groups={r.warnings} />
        </Section>
      )}

      <details>
        <summary className="text-sm">Full report: files, checks, history backup folder</summary>
        <div className="mt-2 flex flex-col gap-3">
          {r.facts.length > 0 && (
            <Section title={r.deviceName}>
              <Facts facts={r.facts} />
            </Section>
          )}
          <Section title={r.history.title}>
            <p className="m-0 text-sm">{r.history.text}</p>
            {r.history.folder !== null && (
              <p className="m-0 font-mono text-xs [overflow-wrap:anywhere]">{r.history.folder}</p>
            )}
            {r.history.files.length > 0 && (
              <ul className={cn(list, "font-mono")} aria-label="Backed-up files">
                {r.history.files.map((f) => (
                  <li key={f}>{f}</li>
                ))}
              </ul>
            )}
          </Section>
        </div>
      </details>
    </>
  );
}
