/**
 * Report a bug, request a feature or send feedback (D34). Bugs include
 * diagnostics by default; "Show what's sent" previews the exact report.
 * Without a relay in the build, or when sending fails, the report can be
 * saved to a file instead.
 */
import type { ReactNode } from "react";

import type { ReportKind } from "../lib/ipc";
import { isComplete, useReport } from "../store/report";
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
import { Field } from "./ui/Field";
import { Input } from "./ui/Input";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Textarea } from "./ui/Textarea";

const KINDS: { id: ReportKind; label: string; describe: string }[] = [
  { id: "bug", label: "Bug", describe: "What happened?" },
  { id: "feature", label: "Feature request", describe: "What would you like reks to do?" },
  { id: "feedback", label: "Feedback", describe: "What's on your mind?" },
];

const block = "m-0 rounded-(--ctl-r-md) border border-border bg-bg-subtle p-2 font-mono text-xs";

export function ReportDialog() {
  const open = useReport((s) => s.open);
  const phase = useReport((s) => s.phase);
  const close = useReport((s) => s.close);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent size="md">
        <DialogHeader className="border-b border-border pt-5 pb-4">
          <DialogTitle className="text-lg">Report a problem or idea</DialogTitle>
          <DialogDescription>
            Goes to the reks developers. Nothing is sent until you press Send.
          </DialogDescription>
        </DialogHeader>
        {phase === "sent" ? <Sent /> : <Form />}
      </DialogContent>
    </Dialog>
  );
}

function Sent() {
  const close = useReport((s) => s.close);
  return (
    <>
      <DialogBody className="py-4 text-sm" role="status">
        Thanks! Your report was sent.
      </DialogBody>
      <DialogFooter className="border-t border-border pt-4">
        <Button variant="primary" onClick={close}>
          Close
        </Button>
      </DialogFooter>
    </>
  );
}

function Form() {
  const draft = useReport((s) => s.draft);
  const phase = useReport((s) => s.phase);
  const canSend = useReport((s) => s.canSend);
  const error = useReport((s) => s.error);
  const notice = useReport((s) => s.notice);
  const { setKind, update, send, saveToFile, close } = useReport.getState();
  const kind = KINDS.find((k) => k.id === draft.kind) ?? KINDS[0];
  const sending = phase === "sending";
  return (
    <>
      <DialogBody className="py-4">
        <SegmentedControl
          aria-label="Report type"
          value={draft.kind}
          onValueChange={(v) => setKind(v as ReportKind)}
          className="self-start"
        >
          {KINDS.map((k) => (
            <Segment key={k.id} value={k.id}>
              {k.label}
            </Segment>
          ))}
        </SegmentedControl>
        <Field label="Title">
          <Input
            value={draft.title}
            maxLength={120}
            onChange={(e) => update({ title: e.target.value })}
            autoFocus
          />
        </Field>
        <Field label={kind?.describe}>
          <Textarea
            rows={4}
            value={draft.description}
            onChange={(e) => update({ description: e.target.value })}
          />
        </Field>
        {draft.kind === "bug" && (
          <>
            <Field label="Steps to reproduce">
              <Textarea
                rows={3}
                placeholder={"1. …\n2. …"}
                value={draft.steps}
                onChange={(e) => update({ steps: e.target.value })}
              />
            </Field>
            <Field label="What did you expect?">
              <Input
                value={draft.expected}
                onChange={(e) => update({ expected: e.target.value })}
              />
            </Field>
          </>
        )}
        {draft.context !== "" && (
          <Labelled label="Error shown">
            <p className={`${block} select-text`}>{draft.context}</p>
          </Labelled>
        )}
        <Field label="Email (optional)" hint="Only if you'd like us to follow up.">
          <Input
            type="email"
            value={draft.contact}
            onChange={(e) => update({ contact: e.target.value })}
          />
        </Field>
        <DiagnosticsChoice />
        {!canSend && (
          <p className="m-0 text-xs text-fg-muted">
            This build can't send reports. Save it to a file and send it to the developers.
          </p>
        )}
        {error !== null && (
          <p className="m-0 text-xs text-danger select-text" role="alert">
            {phase === "failed" ? `Could not send: ${error}` : error}
          </p>
        )}
        {notice !== null && (
          <p className="m-0 text-xs text-fg-muted select-text" role="status">
            {notice}
          </p>
        )}
      </DialogBody>
      <DialogFooter className="border-t border-border pt-4">
        <Button
          variant="ghost"
          className="mr-auto"
          disabled={!isComplete(draft) || sending}
          onClick={() => void saveToFile()}
        >
          Save to file…
        </Button>
        <Button onClick={close}>Cancel</Button>
        {canSend && (
          <Button
            variant="primary"
            disabled={!isComplete(draft) || sending}
            onClick={() => void send()}
          >
            {sending ? "Sending…" : phase === "failed" ? "Retry" : "Send"}
          </Button>
        )}
      </DialogFooter>
    </>
  );
}

function DiagnosticsChoice() {
  const include = useReport((s) => s.draft.includeDiagnostics);
  const preview = useReport((s) => s.preview);
  const loading = useReport((s) => s.previewLoading);
  const { update, loadPreview } = useReport.getState();
  return (
    <div className="flex flex-col gap-2">
      <label className="flex items-start gap-2 text-sm">
        <Checkbox
          className="mt-0.5"
          checked={include}
          onChange={(e) => update({ includeDiagnostics: e.target.checked })}
        />
        <span>
          Include diagnostics
          <span className="block text-xs text-fg-muted">
            App version, OS, library size, the recent log and the last crash. Your home and music
            folder paths are hidden.
          </span>
        </span>
      </label>
      {preview === null ? (
        <Button
          variant="link"
          size="sm"
          className="self-start"
          disabled={loading}
          onClick={() => void loadPreview()}
        >
          {loading ? "Loading…" : "Show what's sent"}
        </Button>
      ) : (
        <Labelled label={preview.title === "" ? "Preview" : `Preview: ${preview.title}`}>
          <pre className={`${block} max-h-60 overflow-auto whitespace-pre-wrap select-text`}>
            {preview.body}
          </pre>
        </Labelled>
      )}
    </div>
  );
}

/** A caption over read-only content (`Field` wraps a control in a <label>). */
function Labelled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs text-fg-muted">{label}</span>
      {children}
    </div>
  );
}
