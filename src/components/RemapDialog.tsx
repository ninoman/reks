/**
 * Change a library folder (phase 1 task 15, D36): the music folder moved, so
 * every track path under it gets the new prefix. The dry run shows counts,
 * a few examples and whether a sample of the files exists at the new place.
 */
import { remapBlocker, useRemap } from "../store/remap";
import { ErrorMessage } from "./ErrorMessage";
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

export function RemapDialog() {
  const open = useRemap((s) => s.step !== "closed");
  if (!open) return null;
  return <Remap />;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function Remap() {
  const s = useRemap();
  const blocker = s.plan === null ? null : remapBlocker(s.plan);
  const busy = s.step === "planning" || s.step === "applying";

  return (
    <Dialog open onOpenChange={(o) => !o && s.close()}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Change library folder</DialogTitle>
          <DialogDescription>
            Points <span className="font-mono select-text">{s.root?.path}</span> at{" "}
            <span className="font-mono select-text">{s.folder}</span>. The files are not touched;
            only the paths in the library change.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-3">
          {busy && (
            <div className="flex flex-col gap-1">
              <p className="m-0 text-fg-muted" role="status">
                {s.step === "planning" ? "Checking the new folder…" : "Changing paths…"}
              </p>
              <Progress aria-label="Progress" />
            </div>
          )}
          {s.plan !== null && s.step !== "report" && !busy && (
            <>
              <p className="m-0 text-sm font-semibold select-text" role="status">
                {plural(s.plan.tracks, "track", "tracks")} will change
                {s.plan.nestedRoots > 0 &&
                  `, with ${plural(s.plan.nestedRoots, "library folder", "library folders")} inside it`}
                {s.plan.sampled > 0 &&
                  ` · ${s.plan.sampleFound} of ${s.plan.sampled} files checked exist at the new place`}
              </p>
              {s.plan.tracks === 0 && (
                <p className="m-0 text-fg-muted">No track in the library is under this folder.</p>
              )}
              {s.plan.examples.length > 0 && (
                <ul
                  className="m-0 flex list-none flex-col gap-1 p-0 text-xs select-text"
                  aria-label="Examples"
                >
                  {s.plan.examples.map((e) => (
                    <li key={e.from} className="odd:bg-row-alt px-2 py-0.5 font-mono">
                      <div className="truncate text-fg-muted" title={e.from}>
                        {e.from}
                      </div>
                      <div className="truncate" title={e.to}>
                        → {e.to}
                      </div>
                    </li>
                  ))}
                </ul>
              )}
              {blocker !== null && <ErrorMessage as="p" className="m-0" message={blocker} />}
              {blocker === null && s.plan.sampleFound < s.plan.sampled && (
                <p className="m-0 text-xs text-warning">
                  Some files are not at the new place. They stay flagged missing after the rescan.
                </p>
              )}
            </>
          )}
          {s.step === "report" && s.report !== null && (
            <p className="m-0 select-text" role="status">
              {plural(s.report.tracks, "track", "tracks")} now point to the new folder. reks is
              rescanning it.
            </p>
          )}
          {s.error !== null && <ErrorMessage as="p" className="m-0" message={s.error} />}
        </DialogBody>
        <DialogFooter>
          {s.step === "report" || s.step === "failed" ? (
            <Button variant="primary" onClick={s.close}>
              {s.step === "report" ? "Done" : "Close"}
            </Button>
          ) : (
            <>
              <Button disabled={s.step === "applying"} onClick={s.close}>
                Cancel
              </Button>
              <Button
                variant="primary"
                disabled={s.step !== "plan" || blocker !== null || s.plan?.tracks === 0}
                onClick={() => void s.apply()}
              >
                Change folder
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
