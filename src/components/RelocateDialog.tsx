/**
 * Find missing files in a folder (phase 1 task 15, D36): the dry run lists
 * sure matches (same content), tracks that need a pick (identical copies, or
 * files that only share name or tags and length, never relinked unless
 * picked) and tracks not found; Apply relinks, keeping cues, grids, playlists
 * and history.
 */
import { formatBytes, formatDuration } from "../lib/format";
import type {
  FoundFile,
  RelocateEvent,
  RelocatePlan,
  RelocateReport,
  RelocateScope,
  ReviewItem,
} from "../lib/ipc";
import { relinkCount, useRelocate } from "../store/relocate";
import { useScan } from "../store/scan";
import { ErrorMessage } from "./ErrorMessage";
import { findMissingFiles } from "./relocateActions";
import { Button } from "./ui/Button";
import { Checkbox } from "./ui/Checkbox";
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
import { ScrollArea } from "./ui/ScrollArea";
import { Select } from "./ui/Select";

export function RelocateDialog() {
  const open = useRelocate((s) => s.step !== "closed");
  if (!open) return null;
  return <Relocate />;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function progressText(e: RelocateEvent | null, applying: boolean): string {
  if (e === null) return applying ? "Relinking…" : "Looking through the folder…";
  switch (e.type) {
    case "walking":
      return `Looking through the folder… ${e.found} files`;
    case "hashing":
      return `Comparing ${e.done} of ${e.total} files that could match…`;
    case "reading":
      return `Reading names and tags… ${e.done} of ${e.total}`;
    case "applying":
      return `Relinking ${e.done} of ${e.total}…`;
  }
}

function scopeText(scope: RelocateScope | null): string {
  if (scope?.kind === "tracks") {
    return scope.ids.length === 1 ? "the selected track" : "the selected tracks";
  }
  return "all missing tracks";
}

function Relocate() {
  const s = useRelocate();
  const close = () => s.close();
  const busy = s.step === "planning" || s.step === "applying";
  const count = s.scan === null ? 0 : relinkCount(s.scan, s.picks);

  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Find missing files</DialogTitle>
          <DialogDescription>
            Looks for {scopeText(s.scope)} in{" "}
            <span className="font-mono select-text">{s.folder}</span>. Only the file location
            changes: cues, grids, tags, playlists and history stay.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-3">
          {busy && (
            <div className="flex flex-col gap-1">
              <p className="m-0 text-fg-muted" role="status">
                {s.stopping ? "Stopping…" : progressText(s.progress, s.step === "applying")}
              </p>
              <Progress
                aria-label="Progress"
                {...(s.progress !== null && "total" in s.progress
                  ? { value: s.progress.done, max: Math.max(1, s.progress.total) }
                  : {})}
              />
            </div>
          )}
          {s.scan !== null && s.step === "plan" && <PlanView plan={s.scan.plan} />}
          {s.step === "report" && s.report !== null && <Done report={s.report} />}
          {s.error !== null && <ErrorMessage as="p" className="m-0" message={s.error} />}
        </DialogBody>
        <DialogFooter>
          <Footer count={count} />
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Footer({ count }: { count: number }) {
  const s = useRelocate();
  const plan = s.scan?.plan;
  if (s.step === "report" || s.step === "failed") {
    const root = s.report?.root;
    return (
      <>
        {root && (
          <Button
            onClick={() => {
              void useScan.getState().scan([root.path]);
              s.close();
            }}
          >
            Scan new folder
          </Button>
        )}
        <Button variant="primary" onClick={s.close}>
          {s.step === "report" ? "Done" : "Close"}
        </Button>
      </>
    );
  }
  if (s.step === "planning" || s.step === "applying") {
    return (
      <Button disabled={s.stopping} onClick={() => void s.cancel()}>
        Stop
      </Button>
    );
  }
  return (
    <>
      {plan !== undefined && plan.lookedFor > 0 && count === 0 && (
        <Button
          onClick={() => {
            const scope = s.scope;
            s.close();
            if (scope !== null) void findMissingFiles(scope);
          }}
        >
          Try another folder…
        </Button>
      )}
      <Button onClick={s.close}>Cancel</Button>
      <Button variant="primary" disabled={count === 0} onClick={() => void s.apply()}>
        {count === 0 ? "Relink" : `Relink ${plural(count, "track", "tracks")}`}
      </Button>
    </>
  );
}

function PlanView({ plan }: { plan: RelocatePlan }) {
  const { picks, pick, addRoot, setAddRoot } = useRelocate();
  if (plan.lookedFor === 0) {
    return <p className="m-0 text-fg-muted">None of these tracks is missing.</p>;
  }
  const picked = Object.keys(picks).length;
  return (
    <>
      <p className="m-0 text-sm font-semibold select-text" role="status">
        {plural(plan.lookedFor, "missing track", "missing tracks")}: {plan.sure.length} found
        {plan.review.length > 0 && `, ${plan.review.length} to review`}
        {plan.notFound.length > 0 && `, ${plan.notFound.length} not found`}
        {plan.filesSeen === 0 && " · no new audio files in this folder"}
      </p>

      {plan.sure.length > 0 && (
        <details open={plan.review.length === 0} className="flex flex-col gap-1 text-xs">
          <summary className="cursor-default text-md">
            Sure matches ({plan.sure.length}) · same content, relinked when you apply
          </summary>
          <ScrollArea className="mt-1 max-h-40">
            <ul
              className="m-0 flex list-none flex-col gap-1 p-0 select-text"
              aria-label="Sure matches"
            >
              {plan.sure.map((m) => (
                <li key={m.track.id} className="odd:bg-row-alt px-2 py-0.5">
                  <div className="truncate text-md">{m.track.label}</div>
                  <div className="truncate font-mono text-fg-muted" title={m.file.path}>
                    {m.file.path}
                  </div>
                </li>
              ))}
            </ul>
          </ScrollArea>
        </details>
      )}

      {plan.review.length > 0 && (
        <section className="flex flex-col gap-1" aria-label="Needs your pick">
          <h3 className="m-0 text-md font-semibold">
            Needs your pick ({picked} of {plan.review.length} chosen)
          </h3>
          <p className="m-0 text-xs text-fg-muted">
            Skipped unless you choose a file. Files that only share a name or tags and length may be
            a different edit of the song.
          </p>
          <ScrollArea className="max-h-56">
            <ul className="m-0 flex list-none flex-col gap-1 p-0">
              {plan.review.map((r) => (
                <ReviewRow
                  key={r.track.id}
                  item={r}
                  value={picks[r.track.id] ?? ""}
                  onPick={(path) => pick(r.track.id, path)}
                />
              ))}
            </ul>
          </ScrollArea>
        </section>
      )}

      {plan.notFound.length > 0 && (
        <details className="flex flex-col gap-1 text-xs">
          <summary className="cursor-default text-md text-warning">
            Not found ({plan.notFound.length})
          </summary>
          <ScrollArea className="mt-1 max-h-32">
            <ul
              className="m-0 flex list-none flex-col gap-0.5 p-0 select-text"
              aria-label="Not found"
            >
              {plan.notFound.map((t) => (
                <li key={t.id} className="px-2">
                  {t.label} <span className="font-mono text-fg-subtle">{t.path}</span>
                </li>
              ))}
            </ul>
          </ScrollArea>
        </details>
      )}

      {plan.problems.length > 0 && (
        <details className="text-xs">
          <summary className="cursor-default text-warning">
            {plural(plan.problems.length, "file or folder", "files or folders")} could not be read
          </summary>
          <ul className="m-0 list-none p-0 select-text" aria-label="Problems">
            {plan.problems.map((p) => (
              <li key={p.path} className="font-mono">
                {p.path}: {p.message}
              </li>
            ))}
          </ul>
        </details>
      )}

      {!plan.insideRoot && plan.sure.length + plan.review.length > 0 && (
        <label className="flex items-center gap-2">
          <Checkbox checked={addRoot} onChange={(e) => setAddRoot(e.target.checked)} />
          Add this folder to the library folders
        </label>
      )}
    </>
  );
}

function candidateText(f: FoundFile): string {
  const bits = [formatBytes(f.size)];
  if (f.durationMs !== null) bits.push(formatDuration(f.durationMs));
  if (!f.sameSize) bits.push("different size");
  return bits.join(" · ");
}

function ReviewRow({
  item,
  value,
  onPick,
}: {
  item: ReviewItem;
  value: string;
  onPick: (path: string | null) => void;
}) {
  return (
    <li className="flex flex-col gap-0.5 odd:bg-row-alt px-2 py-1">
      <div className="truncate text-md select-text">{item.track.label}</div>
      <div className="text-xs text-fg-muted">
        {item.basis === "sameContent"
          ? "Identical copies in several places"
          : "Same name or tags and length, content differs"}
      </div>
      <Select
        size="sm"
        aria-label={`File for ${item.track.label}`}
        value={value}
        onChange={(e) => onPick(e.target.value === "" ? null : e.target.value)}
      >
        <option value="">Skip</option>
        {item.candidates.map((c) => (
          <option key={c.path} value={c.path}>
            {c.path} ({candidateText(c)})
          </option>
        ))}
      </Select>
    </li>
  );
}

function Done({ report }: { report: RelocateReport }) {
  return (
    <div className="flex flex-col gap-1 select-text">
      <p className="m-0" role="status">
        {plural(report.relinked, "track", "tracks")} relinked.
        {report.root !== null && " The folder was added to the library folders."}
      </p>
      {report.skipped.length > 0 && (
        <details className="text-xs" open>
          <summary className="cursor-default text-warning">
            {report.skipped.length} not relinked
          </summary>
          <ul className="m-0 list-none p-0" aria-label="Not relinked">
            {report.skipped.map((k) => (
              <li key={k.track.id}>
                {k.track.label}: {k.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
