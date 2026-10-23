/**
 * Import play history (phase-2.5.md task 9): pick a mounted stick or a
 * folder (a stick backup reks made before an export, or a copy of a stick),
 * see the dry run, then import. The dry run reads the stick and changes
 * nothing; every new session has a Practice checkbox (practice sessions are
 * kept but not counted). All start unticked: every session counts unless the
 * user marks it practice, here or later in the session header. Sessions
 * from the stick's OneLibrary database (read with the OneLibrary key) carry a
 * OneLibrary badge; a database that could not be read is said so.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";

import {
  type DeviceInfo,
  devicesList,
  type HistoryImportReport,
  type HistoryScan,
  historyImport,
  historyScan,
  type OneLibraryStatus,
  type PlannedSession,
  type ScanSource,
} from "../lib/ipc";
import { useHistory } from "../store/history";
import { ErrorMessage } from "./ErrorMessage";
import { Badge } from "./ui/Badge";
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
import { ScrollArea } from "./ui/ScrollArea";

type Step =
  | { kind: "source" }
  | { kind: "scanning" }
  | { kind: "plan"; scan: HistoryScan }
  | { kind: "importing"; scan: HistoryScan }
  | { kind: "done"; report: HistoryImportReport };

export function HistoryImportDialog() {
  const isOpen = useHistory((s) => s.importOpen);
  if (!isOpen) return null;
  return <ImportDialog />;
}

function ImportDialog() {
  const h = useHistory.getState;
  const [step, setStep] = useState<Step>({ kind: "source" });
  const [devices, setDevices] = useState<DeviceInfo[] | null>(null);
  /** Fingerprint → Practice, for the sessions the user changed. */
  const [practice, setPractice] = useState<Record<string, boolean>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    devicesList().then(
      (d) => live && setDevices(d.filter((x) => x.removable)),
      (e: unknown) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, []);

  async function scan(source: ScanSource) {
    setError(null);
    setStep({ kind: "scanning" });
    try {
      setPractice({});
      setStep({ kind: "plan", scan: await historyScan(source) });
    } catch (e) {
      setError(String(e));
      setStep({ kind: "source" });
    }
  }

  async function pickFolder() {
    const dir = await open({ directory: true, title: "Stick or stick backup folder" });
    if (typeof dir === "string") await scan({ kind: "folder", path: dir });
  }

  async function runImport(s: HistoryScan) {
    setError(null);
    setStep({ kind: "importing", scan: s });
    try {
      // Every new session explicitly: unticked means counted, whatever the
      // core's default.
      const excluded = Object.fromEntries(
        s.plan.sessions
          .filter((p) => p.action.kind === "new")
          .map((p) => [p.fingerprint, practice[p.fingerprint] ?? false]),
      );
      const report = await historyImport(s.planId, { excluded });
      setStep({ kind: "done", report });
      await h().imported();
    } catch (e) {
      setError(String(e));
      setStep({ kind: "plan", scan: s });
    }
  }

  const close = () => h().setImportOpen(false);
  const plan = step.kind === "plan" || step.kind === "importing" ? step.scan : null;
  const toImport = plan?.plan.sessions.filter((s) => s.action.kind !== "skip").length ?? 0;

  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Import play history</DialogTitle>
          <DialogDescription>
            Reads the history players wrote to a stick. The stick is not changed.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-2">
          {(step.kind === "source" || step.kind === "scanning") && (
            <SourcePicker
              devices={devices}
              busy={step.kind === "scanning"}
              onStick={(device) => void scan({ kind: "stick", device })}
              onFolder={() => void pickFolder()}
            />
          )}
          {plan !== null && (
            <PlanTable
              scan={plan}
              practice={practice}
              disabled={step.kind === "importing"}
              onPractice={(fp, on) => setPractice((p) => ({ ...p, [fp]: on }))}
            />
          )}
          {step.kind === "done" && <Done report={step.report} />}
          {error !== null && <ErrorMessage message={error} />}
        </DialogBody>
        <DialogFooter>
          {step.kind === "done" ? (
            <Button variant="primary" onClick={close}>
              Done
            </Button>
          ) : (
            <>
              {plan !== null && (
                <Button
                  className="mr-auto"
                  disabled={step.kind === "importing"}
                  onClick={() => setStep({ kind: "source" })}
                >
                  Back
                </Button>
              )}
              <Button onClick={close}>Cancel</Button>
              {plan !== null && (
                <Button
                  variant="primary"
                  disabled={toImport === 0 || step.kind === "importing"}
                  onClick={() => void runImport(plan)}
                >
                  {step.kind === "importing"
                    ? "Importing…"
                    : `Import ${toImport} session${toImport === 1 ? "" : "s"}`}
                </Button>
              )}
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SourcePicker({
  devices,
  busy,
  onStick,
  onFolder,
}: {
  devices: DeviceInfo[] | null;
  busy: boolean;
  onStick: (d: DeviceInfo) => void;
  onFolder: () => void;
}) {
  return (
    <div className="flex flex-col gap-2" aria-busy={busy}>
      <h3 className="m-0 text-sm font-semibold">Read from</h3>
      {devices === null ? (
        <p className="m-0 text-fg-muted">Looking for sticks…</p>
      ) : devices.length === 0 ? (
        <p className="m-0 text-fg-muted">No removable drive is mounted.</p>
      ) : (
        <ul className="m-0 flex flex-col gap-1 p-0">
          {devices.map((d) => (
            <li key={d.mountPoint} className="flex items-center gap-2">
              <span className="flex-1 truncate">
                {d.label || d.osName}{" "}
                <span className="font-mono text-xs text-fg-muted">{d.mountPoint}</span>
              </span>
              <Button size="sm" disabled={busy} onClick={() => onStick(d)}>
                Read history
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-center gap-2">
        <span className="flex-1 text-xs text-fg-muted">
          Or a stick backup reks made before an export (in the app data folder, under
          stick-backups), or a copy of a stick.
        </span>
        <Button size="sm" disabled={busy} onClick={onFolder}>
          Choose folder…
        </Button>
      </div>
      {busy && <p className="m-0 text-fg-muted">Reading history…</p>}
    </div>
  );
}

function actionLabel(s: PlannedSession): string {
  switch (s.action.kind) {
    case "new":
      return "New";
    case "extend":
      return `Adds ${s.entries.length - s.action.existingEntries} entries`;
    case "skip":
      return s.action.reason === "alreadyImported" ? "Already imported" : "Duplicate on stick";
  }
}

function PlanTable({
  scan,
  practice,
  disabled,
  onPractice,
}: {
  scan: HistoryScan;
  practice: Record<string, boolean>;
  disabled: boolean;
  onPractice: (fingerprint: string, on: boolean) => void;
}) {
  const { plan } = scan;
  if (plan.sessions.length === 0) {
    return (
      <>
        <p className="m-0 text-fg-muted">No play history on {scan.source.path}.</p>
        <OneLibraryNote status={scan.onelibrary} />
      </>
    );
  }
  return (
    <>
      <OneLibraryNote status={scan.onelibrary} />
      <p className="m-0 text-xs text-fg-muted">
        {scan.source.kind === "backup" ? "Backup" : "Stick"}{" "}
        <span className="font-mono">{scan.source.path}</span>
        {plan.volumeSerial !== null && ` · serial ${plan.volumeSerial}`}
        {plan.baselineDate !== null && ` · rekordbox baseline ${plan.baselineDate}`}
      </p>
      <ScrollArea className="max-h-80">
        <table className="w-full border-collapse text-md" aria-label="Sessions on the stick">
          <thead>
            <tr className="text-left text-xs text-fg-muted [&>th]:px-2 [&>th]:py-1 [&>th]:font-normal">
              <th>Session</th>
              <th>Played</th>
              <th>Entries</th>
              <th>Action</th>
              <th>Practice</th>
            </tr>
          </thead>
          <tbody>
            {plan.sessions.map((s) => (
              <PlanRow
                key={s.fingerprint}
                s={s}
                practice={practice[s.fingerprint] ?? false}
                disabled={disabled}
                onPractice={(on) => onPractice(s.fingerprint, on)}
              />
            ))}
          </tbody>
        </table>
      </ScrollArea>
    </>
  );
}

/** Says when the stick's `exportLibrary.db` was there but not read. */
function OneLibraryNote({ status }: { status: OneLibraryStatus }) {
  if (status.status !== "notRead") return null;
  return (
    <p className="m-0 text-xs text-fg-muted" role="status">
      The stick's OneLibrary database was not read ({status.reason}); its sessions are not listed.
    </p>
  );
}

function PlanRow({
  s,
  practice,
  disabled,
  onPractice,
}: {
  s: PlannedSession;
  practice: boolean;
  disabled: boolean;
  onPractice: (on: boolean) => void;
}) {
  const tracks = s.entries.filter((e) => e.kind === "track");
  const unmatched = tracks.filter((e) => e.trackId === null).length;
  const dividers = s.entries.length - tracks.length;
  const skip = s.action.kind === "skip";
  return (
    <tr
      className="align-top odd:bg-row-alt data-[skip]:text-fg-subtle [&>td]:px-2 [&>td]:py-1"
      data-skip={skip || undefined}
    >
      <td className="whitespace-nowrap">
        {s.sourceName}
        {s.library === "oneLibrary" && (
          <Badge variant="muted" className="ml-1.5">
            OneLibrary
          </Badge>
        )}
      </td>
      <td className="whitespace-nowrap">{s.playedOn ?? "Date unknown"}</td>
      <td className="text-xs">
        {skip
          ? "–"
          : [
              `${tracks.length} track${tracks.length === 1 ? "" : "s"}`,
              unmatched > 0 && `${unmatched} not in library`,
              dividers > 0 && `${dividers} divider${dividers === 1 ? "" : "s"}`,
            ]
              .filter(Boolean)
              .join(" · ")}
      </td>
      <td className="whitespace-nowrap">{actionLabel(s)}</td>
      <td>
        {s.action.kind === "new" ? (
          <label className="flex items-start gap-1.5">
            <Checkbox
              className="mt-0.5"
              checked={practice}
              disabled={disabled}
              aria-label={`Practice: ${s.sourceName}${s.library === "oneLibrary" ? " (OneLibrary)" : ""}`}
              onChange={(e) => onPractice(e.target.checked)}
            />
          </label>
        ) : s.action.kind === "extend" ? (
          <span className="text-xs text-fg-muted">unchanged</span>
        ) : null}
      </td>
    </tr>
  );
}

function Done({ report }: { report: HistoryImportReport }) {
  const n = report.created.length;
  return (
    <div className="flex flex-col gap-1">
      <p className="m-0">
        Imported {n} new session{n === 1 ? "" : "s"}
        {report.extended.length > 0 && `, extended ${report.extended.length}`}
        {report.skipped > 0 && `, skipped ${report.skipped} already imported`}.
      </p>
      <p className="m-0 text-fg-muted">
        {report.entries} entries
        {report.unmatched > 0 &&
          `; ${report.unmatched} ${report.unmatched === 1 ? "is" : "are"} not in the library: open the session and use Locate… to link ${report.unmatched === 1 ? "it" : "them"}`}
        . Play counts are updated.
      </p>
    </div>
  );
}
