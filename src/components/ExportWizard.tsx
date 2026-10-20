/**
 * USB export wizard (phase 2 task 10): pick a stick, pick playlists, set the
 * options, read the dry run, export, read the report. Planning, progress,
 * wording and totals all come from Rust (`export_plan` → `DryRun`,
 * `export_apply` → `ExportProgress` / `ExportReportView`); this lays them
 * out. Audio issues the converter fixes link to it (`ConvertDialog`, Older
 * players preset). Play history sessions the library has not imported stop
 * the dry run until the user picks Import now, Back up and continue or
 * Continue and discard (the export guard, Phase 2.5 task 8).
 */
import { type ReactNode, useEffect, useState } from "react";

import { cn } from "../lib/cn";
import type {
  AnlzPreference,
  DryRun,
  DryRunFileGroup,
  ExportOptions,
  ExportOutcome,
  ExportReportView,
  Fact,
  HistoryGuardView,
  HistoryPolicy,
  MissingAnalysis,
  NoticeGroup,
  PlaylistNode,
  TargetProfile,
} from "../lib/ipc";
import { useConverter } from "../store/converter";
import { canContinue, coveredIds, EXPORT_STEPS, useExportWizard } from "../store/exportWizard";
import { usePlaylists } from "../store/playlists";
import { useProfiles } from "../store/profiles";
import { useSettings } from "../store/settings";
import { DeviceList } from "./DevicesDialog";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
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
import { Progress } from "./ui/Progress";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Select } from "./ui/Select";
import { Switch } from "./ui/Switch";

const STEP_TITLES = {
  device: "Stick",
  playlists: "Playlists",
  options: "Options",
  dryRun: "Dry run",
  writing: "Writing",
  report: "Report",
} as const;

const TARGETS: { value: TargetProfile; label: string }[] = [
  { value: "Generic", label: "Any player (safest)" },
  { value: "Cdj2000Nxs", label: "CDJ-2000NXS" },
  { value: "Cdj2000Nxs2", label: "CDJ-2000NXS2" },
  { value: "Cdj3000", label: "CDJ-3000" },
  { value: "XdjXz", label: "XDJ-XZ" },
];

/** `exportWithout` is left out: the writer refuses it (baken writes ANLZ for every track). */
const ANLZ_SOURCES: { value: AnlzPreference; label: string }[] = [
  { value: "rekordboxFirst", label: "rekordbox's, then reks's" },
  { value: "reksFirst", label: "reks's, then rekordbox's" },
  { value: "rekordboxOnly", label: "rekordbox's only" },
];

const MISSING: { value: MissingAnalysis; label: string }[] = [
  { value: "skip", label: "Leave them out" },
  { value: "generate", label: "Generate approximate analysis" },
];

/** Indent per tree level, as in the sidebar tree. */
const INDENT = 18;

const facts =
  "m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 [&_dt]:text-fg-muted [&_dd]:m-0 [&_dd]:tabular-nums";
const list = "m-0 mt-1 list-disc pl-4.5 text-xs [overflow-wrap:anywhere]";

export function ExportWizard() {
  const step = useExportWizard((s) => s.step);
  const mountPoint = useExportWizard((s) => s.mountPoint);
  const device = useExportWizard((s) => s.device);
  const checked = useExportWizard((s) => s.checked);
  const options = useExportWizard((s) => s.options);
  const dryRun = useExportWizard((s) => s.dryRun);
  const planning = useExportWizard((s) => s.planning);
  const error = useExportWizard((s) => s.error);
  const nodes = usePlaylists((s) => s.nodes);
  if (step === "closed") return null;
  const w = useExportWizard.getState();
  const index = EXPORT_STEPS.indexOf(step);
  const ready = canContinue({ step, mountPoint, device, checked, options }, nodes);
  const stepError = step === "device" || step === "playlists" || step === "options";

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) w.close();
      }}
    >
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Export to USB</DialogTitle>
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
        <DialogBody className="min-h-60">
          {stepError && error !== null && (
            <p className="m-0 text-danger select-text" role="alert">
              {error}
            </p>
          )}
          {step === "device" && <DeviceStep />}
          {step === "playlists" && <PlaylistsStep nodes={nodes} />}
          {step === "options" && <OptionsStep />}
          {step === "dryRun" && <DryRunStep nodes={nodes} />}
          {step === "writing" && <WritingStep />}
          {step === "report" && <ReportStep />}
        </DialogBody>
        <DialogFooter className="items-center">
          {step === "writing" ? (
            <WritingFooter />
          ) : step === "report" ? (
            <ReportFooter nodes={nodes} />
          ) : (
            <>
              <Button onClick={() => w.close()}>Cancel</Button>
              {index > 0 && <Button onClick={() => w.back()}>Back</Button>}
              {step === "dryRun" ? (
                <Button
                  variant="primary"
                  disabled={planning || dryRun === null || !dryRun.ready}
                  onClick={() => void w.startExport()}
                >
                  Export
                </Button>
              ) : (
                <Button variant="primary" disabled={!ready} onClick={() => void w.next(nodes)}>
                  {step === "options" ? "Dry run" : "Next"}
                </Button>
              )}
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function WritingStep() {
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

const OUTCOME_TONE: Record<ExportOutcome, string> = {
  success: "text-success",
  problems: "text-warning",
  cancelled: "text-fg",
  failed: "text-danger",
};

function ReportStep() {
  const report = useExportWizard((s) => s.report);
  const error = useExportWizard((s) => s.error);
  const eject = useExportWizard((s) => s.eject);
  return (
    <div className="flex flex-col gap-3 select-text">
      {error !== null && (
        <p className="m-0 text-danger" role="alert">
          {error}
        </p>
      )}
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

      {r.warnings.length > 0 && (
        <Section title="Warnings" tone="warning">
          <Notices groups={r.warnings} />
        </Section>
      )}

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
    </>
  );
}

function ReportFooter({ nodes }: { nodes: PlaylistNode[] }) {
  const report = useExportWizard((s) => s.report);
  const eject = useExportWizard((s) => s.eject);
  const w = useExportWizard.getState();
  return (
    <>
      {report?.outcome !== "success" && (
        <Button className="mr-auto" onClick={() => void w.planAgain(nodes)}>
          Plan again
        </Button>
      )}
      <Button
        disabled={eject.state === "ejecting" || eject.state === "done"}
        onClick={() => void w.ejectStick()}
      >
        {eject.state === "ejecting" ? "Ejecting…" : "Eject"}
      </Button>
      <Button variant="primary" onClick={() => w.close()}>
        Done
      </Button>
    </>
  );
}

function DeviceStep() {
  const mountPoint = useExportWizard((s) => s.mountPoint);
  const match = useProfiles((s) => (mountPoint === null ? null : s.mounted[mountPoint]?.match));
  // Once the picked stick is known (now, or after the identify prompt), its
  // profile ticks the playlists.
  useEffect(() => {
    if (match?.type === "known") void useExportWizard.getState().takeProfile(match.device);
  }, [match]);
  return (
    <>
      <p className="m-0 text-fg-muted">
        Which stick? reks remembers the playlists each stick carries. Nothing is written until the
        last step.
      </p>
      <DeviceList
        selected={mountPoint}
        onSelect={(d) => useExportWizard.getState().setDevice(d.mountPoint)}
      />
    </>
  );
}

function PlaylistsStep({ nodes }: { nodes: PlaylistNode[] }) {
  const checked = useExportWizard((s) => s.checked);
  const device = useExportWizard((s) => s.device);
  const covered = coveredIds(nodes, checked);
  if (nodes.length === 0) {
    return <p className="m-0 text-fg-muted">The library has no playlists yet.</p>;
  }
  return (
    <>
      <p className="m-0 text-fg-muted">
        Pick playlists and folders for {device?.name ?? "the stick"}. A folder exports every
        playlist under it, including ones you add later. Unticked playlists are removed from the
        stick. Next saves the ticks.
      </p>
      <Card role="group" aria-label="Playlists" divided={false} className="py-1">
        {nodes.map((n) => {
          const implied = covered.has(n.id);
          return (
            <label
              key={n.id}
              className="flex h-(--row-h) items-center gap-2 pr-2 text-md"
              style={{ paddingLeft: 8 + n.depth * INDENT }}
            >
              <Checkbox
                checked={implied || checked.has(n.id)}
                disabled={implied}
                onChange={() => useExportWizard.getState().toggle(n.id)}
              />
              <span className="flex-1 truncate">{n.name}</span>
              <span className="text-xs text-fg-subtle tabular-nums">
                {n.isFolder ? "folder" : n.entries}
              </span>
            </label>
          );
        })}
      </Card>
    </>
  );
}

function OptionRow({
  title,
  hint,
  badge,
  children,
}: {
  title: string;
  hint: string;
  badge?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3">
      <div className="min-w-0">
        <p className="m-0 flex items-center gap-2 text-sm font-semibold">
          {title}
          {badge}
        </p>
        <p className="m-0 text-xs text-fg-muted">{hint}</p>
      </div>
      {children}
    </div>
  );
}

function OptionsStep() {
  const o = useExportWizard((s) => s.options);
  const keySet = useSettings((s) => s.settings?.onelibraryKeySet ?? false);
  const set = (patch: Partial<ExportOptions>) => useExportWizard.getState().setOptions(patch);
  return (
    <Card>
      <div className="px-4 py-3">
        <Field label="Oldest player the stick must work on">
          <Select
            value={o.target}
            onChange={(e) => set({ target: e.target.value as TargetProfile })}
          >
            {TARGETS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className="px-4 py-3">
        <Field
          label="Analysis to write"
          hint="Waveforms, beatgrid and cues when rekordbox and reks have both analysed a track. A beatgrid you edited or locked uses reks's unless rekordbox's only is chosen. reks's analysis is not yet tested on players. Saved for later exports."
        >
          <Select
            aria-label="Analysis to write"
            value={o.anlzPreference}
            onChange={(e) => set({ anlzPreference: e.target.value as AnlzPreference })}
          >
            {ANLZ_SOURCES.map((a) => (
              <option key={a.value} value={a.value}>
                {a.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className="px-4 py-3">
        <Field
          label="Tracks without analysis"
          hint="Neither source has them. Generated analysis is approximate."
        >
          <Select
            value={o.missingAnalysis}
            onChange={(e) => set({ missingAnalysis: e.target.value as MissingAnalysis })}
          >
            {MISSING.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <OptionRow
        title="Leave out unsupported files"
        hint="Missing or unplayable files are left out instead of blocking the export."
      >
        <Switch
          aria-label="Leave out unsupported files"
          checked={o.skipUnsupported}
          onCheckedChange={(v) => set({ skipUnsupported: v })}
        />
      </OptionRow>
      <OptionRow
        title="Keep stale files"
        hint="Leave music and analysis the export no longer uses on the stick."
      >
        <Switch
          aria-label="Keep stale files"
          checked={o.keepStale}
          onCheckedChange={(v) => set({ keepStale: v })}
        />
      </OptionRow>
      <OptionRow title="Artwork" hint="Write cover art for the players to show.">
        <Switch
          aria-label="Artwork"
          checked={o.artwork}
          onCheckedChange={(v) => set({ artwork: v })}
        />
      </OptionRow>
      <OptionRow
        title="Playlist dividers"
        hint="Write each divider as a few seconds of silence titled with its label. Not yet tested on players."
        badge={
          <Badge variant="muted" title="Not yet tested on a CDJ or XDJ">
            UNVERIFIED
          </Badge>
        }
      >
        <Switch
          aria-label="Playlist dividers"
          checked={o.dividers}
          onCheckedChange={(v) => set({ dividers: v })}
        />
      </OptionRow>
      <OptionRow
        title="OneLibrary"
        hint={`Also write exportLibrary.db, the library newer players read. Not tested on any player and not supported by AlphaTheta.${
          keySet ? "" : " Needs the key: add it in Settings → Experimental."
        }`}
        badge={
          <Badge variant="muted" title="Experimental: not tested on any player">
            EXPERIMENTAL
          </Badge>
        }
      >
        <Switch
          aria-label="OneLibrary"
          checked={o.onelibrary}
          onCheckedChange={(v) => set({ onelibrary: v })}
        />
      </OptionRow>
      <div className="flex flex-col gap-2 px-4 py-3">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <p className="m-0 text-sm font-semibold">Play history on the stick</p>
            <p className="m-0 text-xs text-fg-muted">
              Players log what you played on the stick; exporting erases it.
            </p>
          </div>
          <SegmentedControl
            aria-label="Play history on the stick"
            value={o.history}
            onValueChange={(v) => set({ history: v as ExportOptions["history"] })}
          >
            <Segment value="backUp">Back up first</Segment>
            <Segment value="discard">Discard</Segment>
          </SegmentedControl>
        </div>
        {o.history === "discard" && (
          <label className="flex items-start gap-2 text-sm text-warning">
            <Checkbox
              className="mt-0.5"
              checked={o.discardHistoryConfirmed}
              onChange={(e) => set({ discardHistoryConfirmed: e.target.checked })}
            />
            <span>
              I understand the stick's play history is erased without a backup and can't be
              recovered.
            </span>
          </label>
        )}
      </div>
    </Card>
  );
}

function DryRunStep({ nodes }: { nodes: PlaylistNode[] }) {
  const dryRun = useExportWizard((s) => s.dryRun);
  const planning = useExportWizard((s) => s.planning);
  const error = useExportWizard((s) => s.error);
  if (planning) {
    return (
      <div className="flex flex-col gap-2">
        <p className="m-0">Planning: reading the stick and checking your files…</p>
        <Progress aria-label="Planning" />
      </div>
    );
  }
  if (error !== null) {
    return (
      <div className="flex flex-col items-start gap-2">
        <p className="m-0 text-danger select-text" role="alert">
          {error}
        </p>
        <Button onClick={() => void useExportWizard.getState().replan(nodes)}>Try again</Button>
      </div>
    );
  }
  if (dryRun === null) return null;
  return <DryRunView dryRun={dryRun} nodes={nodes} />;
}

function DryRunView({ dryRun: d, nodes }: { dryRun: DryRun; nodes: PlaylistNode[] }) {
  return (
    <div className="flex flex-col gap-3 select-text">
      <div className="flex items-start justify-between gap-4">
        <p
          className={d.ready ? "m-0 font-semibold text-success" : "m-0 font-semibold text-danger"}
          role={d.ready ? "status" : "alert"}
        >
          {d.status}
        </p>
        <Button
          size="sm"
          className="shrink-0"
          onClick={() => void useExportWizard.getState().replan(nodes)}
        >
          Plan again
        </Button>
      </div>
      <p className="m-0 text-xs text-fg-muted">
        {d.device.name} · {d.device.fsText} · {d.device.spaceText} · target {d.target}
      </p>

      {d.problems.length > 0 && (
        <Section title="Must be fixed first" tone="danger">
          <Notices groups={d.problems} />
        </Section>
      )}

      <Section title="Space">
        <Progress
          max={Math.max(d.space.totalBytes, 1)}
          value={d.space.usedAfterBytes}
          aria-label="Stick space used after the export"
        />
        <Facts facts={d.space.facts} />
        {!d.space.fits && (
          <p className="m-0 text-xs text-danger">The new files don't fit in today's free space.</p>
        )}
      </Section>

      <Section title="Export">
        <Facts facts={d.summary} />
      </Section>

      {d.history.guard !== null && (
        <HistoryGuard guard={d.history.guard} policy={d.history.policy} nodes={nodes} />
      )}

      <Section title={d.history.title} tone={d.history.atRisk ? "warning" : undefined}>
        <p className="m-0 text-sm">{d.history.text}</p>
        {d.history.actionText !== null && <p className="m-0 text-sm">{d.history.actionText}</p>}
        <Files label="Backed up" group={d.history.backups} />
      </Section>

      {d.warnings.length > 0 && (
        <Section title="Warnings" tone="warning">
          <Notices groups={d.warnings} />
        </Section>
      )}

      {d.convertText !== null && (
        <Section title="Convert for older players">
          <p className="m-0 text-sm">{d.convertText}</p>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              onClick={() => useConverter.getState().open(d.convertible, "olderPlayers")}
            >
              Convert…
            </Button>
            <span className="text-xs text-fg-muted">Plan again after converting.</span>
          </div>
        </Section>
      )}

      {d.onelibrary !== null && (
        <Section title={d.onelibrary.title} tone="warning">
          <p className="m-0 text-sm">{d.onelibrary.text}</p>
          <p className="m-0 font-mono text-xs [overflow-wrap:anywhere]">
            Write {d.onelibrary.write.path}{" "}
            <span className="text-fg-muted">({d.onelibrary.write.sizeText})</span>
          </p>
          <Files label="Remove" group={d.onelibrary.removes} />
        </Section>
      )}

      <Section title="Files">
        <Files label="Copy" group={d.copies} />
        <Files label="Delete" group={d.deletes} />
        <details>
          <summary className="text-sm">Playlists on the stick</summary>
          <ul className="m-0 mt-1 list-none p-0 text-xs">
            {d.playlists.map((p) => (
              <li key={p.id} style={{ paddingLeft: p.depth * INDENT }}>
                {p.name}
                {p.isFolder ? "/" : ""}
                {p.detail !== "" && <span className="text-fg-muted"> · {p.detail}</span>}
              </li>
            ))}
          </ul>
        </details>
      </Section>
    </div>
  );
}

const GUARD_CHOICES: Record<HistoryPolicy, string> = {
  import: "Import now",
  backUp: "Back up and continue",
  discard: "Continue and discard",
};

/** The export guard: what happens to the sessions the library has not imported. */
function HistoryGuard({
  guard: g,
  policy,
  nodes,
}: {
  guard: HistoryGuardView;
  policy: HistoryPolicy;
  nodes: PlaylistNode[];
}) {
  const [confirming, setConfirming] = useState(false);
  const [understood, setUnderstood] = useState(false);
  const choose = (p: HistoryPolicy) => {
    setConfirming(false);
    setUnderstood(false);
    void useExportWizard.getState().chooseHistory(p, nodes);
  };
  return (
    <Section title={g.title} tone={g.chosen ? undefined : "danger"}>
      <p className="m-0 text-sm">{g.text}</p>
      {g.sessions.length > 0 && (
        <ul className="m-0 list-disc pl-5 text-xs">
          {g.sessions.map((s) => (
            <li key={s}>{s}</li>
          ))}
        </ul>
      )}
      {g.chosen && <p className="m-0 text-sm font-semibold">You chose: {GUARD_CHOICES[policy]}.</p>}
      <div className="flex flex-wrap items-center gap-2">
        {g.canImport && (
          <Button
            size="sm"
            variant={g.chosen ? "secondary" : "primary"}
            disabled={g.chosen && policy === "import"}
            onClick={() => choose("import")}
          >
            {GUARD_CHOICES.import}
          </Button>
        )}
        <Button
          size="sm"
          variant={!g.canImport && !g.chosen ? "primary" : "secondary"}
          disabled={g.chosen && policy === "backUp"}
          onClick={() => choose("backUp")}
        >
          {GUARD_CHOICES.backUp}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          disabled={confirming || (g.chosen && policy === "discard")}
          onClick={() => setConfirming(true)}
        >
          {GUARD_CHOICES.discard}…
        </Button>
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
              disabled={!understood}
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

function Section({
  title,
  tone,
  children,
}: {
  title: string;
  tone?: "danger" | "warning" | undefined;
  children: ReactNode;
}) {
  const titleClass =
    tone === "danger" ? "text-danger" : tone === "warning" ? "text-warning" : "text-fg";
  return (
    <section aria-label={title}>
      <Card className="flex flex-col gap-2 px-4 py-3" divided={false}>
        <h3 className={cn("m-0 text-sm font-semibold", titleClass)}>{title}</h3>
        {children}
      </Card>
    </section>
  );
}

function Facts({ facts: rows }: { facts: Fact[] }) {
  return (
    <dl className={facts}>
      {rows.map((f) => (
        <div key={f.label} className="contents">
          <dt>{f.label}</dt>
          <dd>{f.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Notices({ groups }: { groups: NoticeGroup[] }) {
  return (
    <div className="flex flex-col gap-2">
      {groups.map((g) =>
        g.items.length === 0 ? (
          <p key={g.kind} className="m-0 text-sm">
            {g.title}
          </p>
        ) : (
          <details key={g.kind} open={g.items.length <= 3}>
            <summary className="text-sm">{g.title}</summary>
            <ul className={list}>
              {g.items.map((item, i) => (
                // Items of one group can repeat (same message); order is stable.
                // biome-ignore lint/suspicious/noArrayIndexKey: see above
                <li key={i}>{item}</li>
              ))}
            </ul>
          </details>
        ),
      )}
    </div>
  );
}

function Files({ label, group }: { label: string; group: DryRunFileGroup }) {
  if (group.count === 0) return null;
  return (
    <details>
      <summary className="text-sm">
        {label}: {group.totalText}
      </summary>
      <ul className={cn(list, "font-mono")}>
        {group.files.map((f) => (
          <li key={f.path} title={f.from ?? undefined}>
            {f.path} <span className="text-fg-muted">({f.sizeText})</span>
          </li>
        ))}
      </ul>
    </details>
  );
}
