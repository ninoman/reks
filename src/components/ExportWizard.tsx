/**
 * USB export (phase 2 task 10, D42): Review → Writing → Done for one stick,
 * opened from the Sticks view (where its playlists are ticked); the options
 * are Settings › Export. This lays out the steps (`export/`) and the footer.
 * It shares one dialog with the Sticks view (`SticksDialog`), so going
 * between them resizes the dialog instead of swapping it. While a fix runs in
 * another dialog (find files, convert, analyse, settings) it is hidden, and
 * the review plans again afterwards.
 */
import { useDevices } from "../store/devices";
import { EXPORT_STEPS, type ExportStep, useExportWizard } from "../store/exportWizard";
import { ReviewStep } from "./export/ReviewStep";
import { DoneStep, WritingStep } from "./export/WritingDoneSteps";
import { Button } from "./ui/Button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";

const STEP_TITLES: Record<Exclude<ExportStep, "closed">, string> = {
  review: "Review",
  writing: "Writing",
  done: "Done",
};

/** The export's header, steps and footer; `SticksDialog` hosts it. */
export function ExportPanel() {
  const step = useExportWizard((s) => s.step);
  const device = useExportWizard((s) => s.device);
  if (step === "closed") return null;
  const index = EXPORT_STEPS.indexOf(step);

  return (
    <>
      <DialogHeader>
        <DialogTitle>Sync {device?.name ?? "stick"}</DialogTitle>
        <DialogDescription>
          Step {index + 1} of {EXPORT_STEPS.length}: {STEP_TITLES[step]}
        </DialogDescription>
        <ol className="m-0 flex list-none gap-1 p-0" aria-label="Steps">
          {EXPORT_STEPS.map((s, i) => (
            <li
              key={s}
              aria-current={s === step ? "step" : undefined}
              data-done={i < index || undefined}
              className="flex-1 border-t-2 border-border pt-1 text-xs text-fg-subtle aria-[current=step]:border-accent aria-[current=step]:text-fg data-[done]:border-fg-subtle"
            >
              {STEP_TITLES[s]}
            </li>
          ))}
        </ol>
      </DialogHeader>
      <DialogBody>
        {step === "review" && <ReviewStep />}
        {step === "writing" && <WritingStep />}
        {step === "done" && <DoneStep />}
      </DialogBody>
      <DialogFooter className="items-center">
        {step === "review" && <ReviewFooter />}
        {step === "writing" && <WritingFooter />}
        {step === "done" && <DoneFooter />}
      </DialogFooter>
    </>
  );
}

function backToSticks() {
  useExportWizard.getState().close();
  useDevices.getState().setDialogOpen(true);
}

function ReviewFooter() {
  const dryRun = useExportWizard((s) => s.dryRun);
  const planning = useExportWizard((s) => s.planning);
  return (
    <>
      <Button onClick={backToSticks}>Back to Sticks</Button>
      <Button
        variant="primary"
        disabled={planning || dryRun === null || !dryRun.ready}
        onClick={() => void useExportWizard.getState().startExport()}
      >
        {dryRun?.overview.exportLabel ?? "Export"}
      </Button>
    </>
  );
}

function WritingFooter() {
  const progress = useExportWizard((s) => s.progress);
  const cancelling = useExportWizard((s) => s.cancelling);
  return (
    <Button
      disabled={cancelling || progress?.cancellable === false}
      onClick={() => void useExportWizard.getState().cancelExport()}
    >
      Cancel export
    </Button>
  );
}

function DoneFooter() {
  const report = useExportWizard((s) => s.report);
  const eject = useExportWizard((s) => s.eject);
  const w = useExportWizard.getState();
  const ok = report?.outcome === "success";
  const ejected = eject.state === "done";
  return (
    <>
      <Button variant={ok && ejected ? "primary" : "secondary"} onClick={() => w.close()}>
        Close
      </Button>
      {!ok && (
        <Button variant="primary" onClick={() => void w.backToReview()}>
          Back to review
        </Button>
      )}
      {!ejected && (
        <Button
          variant={ok ? "primary" : "secondary"}
          disabled={eject.state === "ejecting"}
          onClick={() => void w.ejectStick()}
        >
          {eject.state === "ejecting" ? "Ejecting…" : "Eject"}
        </Button>
      )}
    </>
  );
}
