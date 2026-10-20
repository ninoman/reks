/**
 * Adopt a foreign stick (phase-2.5.md task 12, stick-profiles.md §4.3): the
 * dry run of a stick reks did not write, a choice per stick playlist (Create
 * under `From stick/<stick>/`, Link to an identical library playlist, or
 * Skip), then apply. The stick is only read. Play history on it is imported
 * first. Every summary and note is worded in Rust.
 */
import { useEffect, useState } from "react";

import {
  type AdoptChoice,
  type AdoptPlaylist,
  type AdoptReport,
  type AdoptScan,
  adoptApply,
  adoptScan,
} from "../lib/ipc";
import { type AdoptTarget, useAdopt } from "../store/adopt";
import { useHistory } from "../store/history";
import { useLibrary } from "../store/library";
import { usePlaylists } from "../store/playlists";
import { useProfiles } from "../store/profiles";
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
import { ScrollArea } from "./ui/ScrollArea";
import { Select } from "./ui/Select";

type Step =
  | { kind: "scanning" }
  | { kind: "failed" }
  | { kind: "plan"; scan: AdoptScan }
  | { kind: "applying"; scan: AdoptScan }
  | { kind: "done"; report: AdoptReport };

export function AdoptDialog() {
  const target = useAdopt((s) => s.target);
  if (target === null) return null;
  return <Adopt key={target.info.mountPoint} target={target} />;
}

/** `<select>` value of a choice. */
function choiceValue(c: AdoptChoice): string {
  return c.kind === "link" ? `link:${c.playlistId}` : c.kind;
}

function parseChoice(v: string): AdoptChoice {
  if (v.startsWith("link:")) return { kind: "link", playlistId: Number(v.slice(5)) };
  return v === "skip" ? { kind: "skip" } : { kind: "create" };
}

function Adopt({ target }: { target: AdoptTarget }) {
  const [step, setStep] = useState<Step>({ kind: "scanning" });
  const [choices, setChoices] = useState<Record<number, AdoptChoice>>({});
  const [error, setError] = useState<string | null>(null);
  const close = () => useAdopt.getState().close();

  useEffect(() => {
    let live = true;
    adoptScan(target.info).then(
      (scan) => live && setStep({ kind: "plan", scan }),
      (e: unknown) => {
        if (live) {
          setError(String(e));
          setStep({ kind: "failed" });
        }
      },
    );
    return () => {
      live = false;
    };
  }, [target]);

  async function apply(scan: AdoptScan) {
    setError(null);
    setStep({ kind: "applying", scan });
    try {
      const report = await adoptApply(scan.planId, {
        deviceId: target.deviceId,
        playlists: choices,
        history: { excluded: {} },
      });
      setStep({ kind: "done", report });
      await Promise.all([
        usePlaylists.getState().load(),
        useLibrary.getState().loadFacets(),
        useHistory.getState().imported(),
      ]);
      useProfiles.getState().select(target.deviceId);
    } catch (e) {
      setError(String(e));
      setStep({ kind: "plan", scan });
    }
  }

  const scan = step.kind === "plan" || step.kind === "applying" ? step.scan : null;

  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Adopt “{target.deviceName}”</DialogTitle>
          <DialogDescription>
            Keeps this stick's playlists as library playlists. The stick is only read; nothing on it
            changes until you sync it.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-3">
          {step.kind === "scanning" && (
            <div className="flex flex-col gap-1">
              <p className="m-0 text-fg-muted">Reading the stick…</p>
              <Progress aria-label="Reading the stick" />
            </div>
          )}
          {scan !== null && (
            <PlanView
              scan={scan}
              deviceName={target.deviceName}
              choices={choices}
              disabled={step.kind === "applying"}
              onChoice={(pdbId, c) => setChoices((s) => ({ ...s, [pdbId]: c }))}
            />
          )}
          {step.kind === "done" && <Done report={step.report} />}
          {error !== null && (
            <p className="m-0 text-danger select-text" role="alert">
              {error}
            </p>
          )}
        </DialogBody>
        <DialogFooter>
          {step.kind === "done" || step.kind === "failed" ? (
            <Button variant="primary" onClick={close}>
              {step.kind === "done" ? "Done" : "Close"}
            </Button>
          ) : (
            <>
              <Button onClick={close}>Cancel</Button>
              <Button
                variant="primary"
                disabled={scan === null || step.kind === "applying"}
                onClick={() => scan !== null && void apply(scan)}
              >
                {step.kind === "applying" ? "Adopting…" : "Adopt"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function PlanView({
  scan,
  deviceName,
  choices,
  disabled,
  onChoice,
}: {
  scan: AdoptScan;
  deviceName: string;
  choices: Record<number, AdoptChoice>;
  disabled: boolean;
  onChoice: (pdbId: number, c: AdoptChoice) => void;
}) {
  const { plan } = scan;
  return (
    <>
      <div className="flex flex-col gap-1 select-text">
        <p className="m-0 text-sm font-semibold" role="status">
          {plan.summary}
        </p>
        <p className="m-0 text-xs text-fg-muted">{plan.historyText}</p>
        {plan.notes.length > 0 && (
          <ul className="m-0 list-disc pl-4.5 text-xs text-warning" aria-label="Notes">
            {plan.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        )}
      </div>
      {plan.playlists.length === 0 ? (
        <p className="m-0 text-fg-muted">The stick holds no playlists.</p>
      ) : (
        <ScrollArea className="max-h-80">
          <table className="w-full border-collapse text-md" aria-label="Playlists on the stick">
            <thead>
              <tr className="text-left text-xs text-fg-muted [&>th]:px-2 [&>th]:py-1 [&>th]:font-normal">
                <th>Playlist on the stick</th>
                <th>Tracks</th>
                <th>In your library</th>
              </tr>
            </thead>
            <tbody>
              {plan.playlists.map((p) => (
                <PlaylistRow
                  key={p.pdbId}
                  p={p}
                  deviceName={deviceName}
                  choice={choices[p.pdbId] ?? p.defaultChoice}
                  disabled={disabled}
                  onChoice={(c) => onChoice(p.pdbId, c)}
                />
              ))}
            </tbody>
          </table>
        </ScrollArea>
      )}
      {plan.unmatched.length > 0 && <Unmatched text={plan.unmatchedText} plan={plan} />}
    </>
  );
}

function PlaylistRow({
  p,
  deviceName,
  choice,
  disabled,
  onChoice,
}: {
  p: AdoptPlaylist;
  deviceName: string;
  choice: AdoptChoice;
  disabled: boolean;
  onChoice: (c: AdoptChoice) => void;
}) {
  return (
    <tr className="align-top odd:bg-row-alt [&>td]:px-2 [&>td]:py-1">
      <td className="select-text">{p.pathText}</td>
      <td className="text-xs text-fg-muted select-text">{p.text}</td>
      <td>
        <Select
          size="sm"
          aria-label={`What to do with ${p.pathText}`}
          disabled={disabled}
          value={choiceValue(choice)}
          onChange={(e) => onChoice(parseChoice(e.target.value))}
        >
          <option value="create">Create in From stick / {deviceName}</option>
          {p.link.map((c) => (
            <option key={c.playlistId} value={`link:${c.playlistId}`}>
              Link to {c.pathText}
            </option>
          ))}
          <option value="skip">Skip (removed at the first sync)</option>
        </Select>
      </td>
    </tr>
  );
}

function Unmatched({ text, plan }: { text: string; plan: AdoptScan["plan"] }) {
  return (
    <details className="flex flex-col gap-1 text-xs">
      <summary className="cursor-default text-warning">{text}</summary>
      <ul
        className="m-0 flex list-none flex-col gap-0.5 p-0 select-text"
        aria-label="Not in your library"
      >
        {plan.unmatched.map((t, i) => (
          // Two rows can share every field (a missing track row has none).
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <li key={i}>
            {t.artist ? `${t.artist} – ` : ""}
            {t.title ?? "Unknown track"}
            {t.stickPath && <span className="font-mono text-fg-subtle"> {t.stickPath}</span>}
          </li>
        ))}
      </ul>
    </details>
  );
}

function Done({ report }: { report: AdoptReport }) {
  return (
    <div className="flex flex-col gap-1 select-text">
      <p className="m-0" role="status">
        {report.summary}
      </p>
      <p className="m-0 text-fg-muted">
        The adopted playlists are ticked in the stick's profile. Sync it from USB devices when you
        are ready.
      </p>
    </div>
  );
}
