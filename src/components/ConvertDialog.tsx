/**
 * The file converter (phase 2.6 task 6): a preset and the settings table,
 * the dry run, progress with Cancel (the dialog can be hidden meanwhile; the
 * app header shows the progress), then the report. Planning, wording and
 * totals come from Rust (`convert_plan` → `ConvertDryRun`, `convert_apply`
 * → `ConvertReportView`); this lays them out.
 */
import { open } from "@tauri-apps/plugin-dialog";
import type { ReactNode } from "react";

import { cn } from "../lib/cn";
import type { ConvertDepth, ConvertFact, ConvertSkip, ConvertTarget } from "../lib/ipc";
import { type Preset, presetOf, type RateChoice, useConverter } from "../store/converter";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
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
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Select } from "./ui/Select";

const PRESET_HINTS: Record<Preset, string> = {
  olderPlayers:
    "AIFF at the source's bit depth (32-bit and float become 24-bit), 88.2 and 96 kHz down to 44.1 and 48 kHz.",
  saveSpace: "FLAC at the source's bit depth and sample rate: lossless, about half the size.",
};

const TARGETS: { value: ConvertTarget; label: string }[] = [
  { value: "wav", label: "WAV" },
  { value: "aiff", label: "AIFF" },
  { value: "flac", label: "FLAC" },
];

const DEPTHS: { value: ConvertDepth; label: string }[] = [
  { value: "keep", label: "Keep (max 24-bit)" },
  { value: "bits16", label: "16-bit" },
  { value: "bits24", label: "24-bit" },
];

const RATES: { value: RateChoice; label: string }[] = [
  { value: "keep", label: "Keep" },
  { value: "keepMax48", label: "Keep, max 48 kHz" },
  { value: "hz44100", label: "44.1 kHz" },
  { value: "hz48000", label: "48 kHz" },
];

const facts =
  "m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 [&_dt]:text-fg-muted [&_dd]:m-0 [&_dd]:tabular-nums";
const list = "m-0 mt-1 list-disc pl-4.5 text-xs [overflow-wrap:anywhere]";

function plural(n: number, word: string): string {
  return `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
}

export function ConvertDialog() {
  const step = useConverter((s) => s.step);
  const hidden = useConverter((s) => s.hidden);
  const count = useConverter((s) => s.trackIds.length);
  if (step === "closed" || hidden) return null;
  const c = useConverter.getState();
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) c.close();
      }}
    >
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Convert files</DialogTitle>
          <DialogDescription>
            {plural(count, "track")} selected · lossless sources only; originals are never
            overwritten
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="min-h-60">
          {step === "settings" && <SettingsStep />}
          {step === "dryRun" && <DryRunStep />}
          {step === "converting" && <ConvertingStep />}
          {step === "report" && <ReportStep />}
        </DialogBody>
        <DialogFooter className="items-center">
          <Footer />
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Footer() {
  const step = useConverter((s) => s.step);
  const dryRun = useConverter((s) => s.dryRun);
  const planning = useConverter((s) => s.planning);
  const stopping = useConverter((s) => s.stopping);
  const c = useConverter.getState();
  switch (step) {
    case "settings":
      return (
        <>
          <Button onClick={() => c.close()}>Cancel</Button>
          <Button variant="primary" onClick={() => void c.plan()}>
            Dry run
          </Button>
        </>
      );
    case "dryRun":
      return (
        <>
          <Button onClick={() => c.close()}>Cancel</Button>
          <Button onClick={() => c.back()}>Back</Button>
          <Button
            variant="primary"
            disabled={planning || dryRun === null || !dryRun.ready}
            onClick={() => void c.start()}
          >
            Convert
          </Button>
        </>
      );
    case "converting":
      return (
        <>
          <Button onClick={() => c.setHidden(true)}>Hide</Button>
          <Button disabled={stopping} onClick={() => void c.cancel()}>
            {stopping ? "Stopping…" : "Cancel"}
          </Button>
        </>
      );
    case "report":
      return (
        <Button variant="primary" onClick={() => c.close()}>
          Done
        </Button>
      );
    default:
      return null;
  }
}

function Row({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3">
      <div className="min-w-0">
        <p className="m-0 text-sm font-semibold">{title}</p>
        {hint !== undefined && <p className="m-0 text-xs text-fg-muted">{hint}</p>}
      </div>
      {children}
    </div>
  );
}

function SettingsStep() {
  const s = useConverter((st) => st.settings);
  const c = useConverter.getState();
  const preset = presetOf(s);

  async function chooseFolder() {
    const dir = await open({ directory: true, title: "Folder for the converted files" });
    if (typeof dir === "string") c.set({ folder: dir });
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <div className="flex items-center justify-between gap-4">
          <p className="m-0 text-sm font-semibold">Preset</p>
          <SegmentedControl
            aria-label="Preset"
            value={preset ?? ""}
            onValueChange={(v) => c.applyPreset(v as Preset)}
          >
            <Segment value="olderPlayers">Older players</Segment>
            <Segment value="saveSpace">Save space</Segment>
          </SegmentedControl>
        </div>
        <p className="m-0 text-xs text-fg-muted">
          {preset === null ? "Custom settings." : PRESET_HINTS[preset]}
        </p>
      </div>
      <Card>
        <Row title="Format">
          <Select
            aria-label="Format"
            value={s.target}
            onChange={(e) => c.set({ target: e.target.value as ConvertTarget })}
          >
            {TARGETS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </Select>
        </Row>
        <Row title="Bit depth" hint="Going to 16-bit applies TPDF dither.">
          <Select
            aria-label="Bit depth"
            value={s.depth}
            onChange={(e) => c.set({ depth: e.target.value as ConvertDepth })}
          >
            {DEPTHS.map((d) => (
              <option key={d.value} value={d.value}>
                {d.label}
              </option>
            ))}
          </Select>
        </Row>
        <Row title="Sample rate">
          <Select
            aria-label="Sample rate"
            value={s.rate}
            onChange={(e) => c.set({ rate: e.target.value as RateChoice })}
          >
            {RATES.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
              </option>
            ))}
          </Select>
        </Row>
        <div className="flex flex-col gap-1 px-4 py-3">
          <div className="flex items-center justify-between gap-4">
            <p className="m-0 text-sm font-semibold">Destination</p>
            <SegmentedControl
              aria-label="Destination"
              value={s.folder === null ? "next" : "folder"}
              onValueChange={(v) => {
                if (v === "next") c.set({ folder: null });
                else void chooseFolder();
              }}
            >
              <Segment value="next">Next to the original</Segment>
              <Segment value="folder">Chosen folder…</Segment>
            </SegmentedControl>
          </div>
          {s.folder !== null && (
            <p className="m-0 text-xs text-fg-muted [overflow-wrap:anywhere] select-text">
              {s.folder}{" "}
              <Button variant="link" size="sm" onClick={() => void chooseFolder()}>
                Change…
              </Button>
            </p>
          )}
        </div>
        <Row
          title="Library"
          hint={
            s.library === "replace"
              ? "The track points at the new file; cues, grid, playlists and history stay."
              : "A new track with the same metadata, cues and grid."
          }
        >
          <SegmentedControl
            aria-label="Library"
            value={s.library}
            onValueChange={(v) => c.set({ library: v as typeof s.library })}
          >
            <Segment value="replace">Replace</Segment>
            <Segment value="addAsNew">Add as new</Segment>
          </SegmentedControl>
        </Row>
        <Row
          title="Originals"
          hint={
            s.library === "replace"
              ? "Moved only after the new file is verified; never deleted."
              : "Kept: the original track still points at them."
          }
        >
          <SegmentedControl
            aria-label="Originals"
            value={s.originals}
            disabled={s.library !== "replace"}
            onValueChange={(v) => c.set({ originals: v as typeof s.originals })}
          >
            <Segment value="keep">Keep</Segment>
            <Segment value="trash">Move to Trash</Segment>
          </SegmentedControl>
        </Row>
      </Card>
    </div>
  );
}

function DryRunStep() {
  const dryRun = useConverter((s) => s.dryRun);
  const planning = useConverter((s) => s.planning);
  const error = useConverter((s) => s.error);
  if (planning) {
    return (
      <div className="flex flex-col gap-2">
        <p className="m-0">Planning: reading the files…</p>
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
        <Button onClick={() => void useConverter.getState().plan()}>Try again</Button>
      </div>
    );
  }
  if (dryRun === null) return null;
  const d = dryRun;
  return (
    <div className="flex flex-col gap-3 select-text">
      <p
        className={cn("m-0 font-semibold", d.ready ? "text-success" : "text-fg-muted")}
        role="status"
      >
        {d.status}
      </p>
      <Section title="Settings">
        <Facts facts={d.facts} />
      </Section>
      {d.warnings.length > 0 && (
        <Section title="Warnings" tone="warning">
          <ul className={list}>
            {d.warnings.map((w, i) => (
              // Warnings have no id; their order is stable for one dry run.
              // biome-ignore lint/suspicious/noArrayIndexKey: see above
              <li key={i}>{w}</li>
            ))}
          </ul>
        </Section>
      )}
      {d.items.length > 0 && (
        <Section title={`Convert: ${plural(d.items.length, "file")}`}>
          <ul className="m-0 flex list-none flex-col gap-2 p-0 text-xs">
            {d.items.map((it) => (
              <li key={it.trackId} className="flex flex-col gap-0.5">
                <span className="text-sm">{it.label}</span>
                <span className="text-fg-muted">
                  {it.from} → {it.to} · {it.sizeText}
                </span>
                <span
                  className="font-mono text-fg-muted [overflow-wrap:anywhere]"
                  title={it.source}
                >
                  {it.dest}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      )}
      <Skipped rows={d.skipped} />
    </div>
  );
}

function ConvertingStep() {
  const p = useConverter((s) => s.progress);
  const stopping = useConverter((s) => s.stopping);
  const total = Math.max(p?.total ?? 1, 1);
  return (
    <div className="flex flex-col gap-2">
      <p className="m-0" role="status">
        {stopping
          ? "Stopping after cleaning up the current file…"
          : p === null || p.label === ""
            ? "Starting…"
            : `File ${p.index + 1} of ${p.total}: ${p.label}`}
      </p>
      <Progress
        max={total}
        value={p === null ? 0 : p.index + p.fraction}
        aria-label="Conversion progress"
      />
      {p !== null && p.failed > 0 && (
        <p className="m-0 text-sm text-warning">{plural(p.failed, "file")} failed so far</p>
      )}
      <p className="m-0 text-xs text-fg-muted">
        Each file is written next to its destination, read back and checked before it takes over the
        track. Cancel leaves the current track as it was.
      </p>
    </div>
  );
}

function ReportStep() {
  const r = useConverter((s) => s.report);
  if (r === null) return null;
  const tone = r.outcome === "success" ? "text-success" : "text-warning";
  return (
    <div className="flex flex-col gap-3 select-text">
      <div>
        <p className={cn("m-0 font-semibold", tone)} role="status">
          {r.title}
        </p>
        <p className="m-0 text-sm">{r.status}</p>
      </div>
      <Facts facts={r.facts} />
      {r.failed.length > 0 && (
        <Section title={`Failed: ${plural(r.failed.length, "file")}`} tone="danger">
          <ul className={list}>
            {r.failed.map((f) => (
              <li key={f.trackId}>
                {f.label}: {f.message}
              </li>
            ))}
          </ul>
        </Section>
      )}
      {r.converted.length > 0 && (
        <Section title={`Converted: ${plural(r.converted.length, "file")}`}>
          <ul className="m-0 flex list-none flex-col gap-2 p-0 text-xs">
            {r.converted.map((c) => (
              <li key={c.newTrackId} className="flex flex-col gap-0.5">
                <span className="text-sm">{c.label}</span>
                <span className="font-mono [overflow-wrap:anywhere]">{c.dest}</span>
                <span className="text-fg-muted [overflow-wrap:anywhere]">{c.original}</span>
              </li>
            ))}
          </ul>
        </Section>
      )}
      <Skipped rows={r.skipped} />
    </div>
  );
}

function Skipped({ rows }: { rows: ConvertSkip[] }) {
  if (rows.length === 0) return null;
  return (
    <Section title={`Skipped: ${plural(rows.length, "track")}`}>
      <details open={rows.length <= 3}>
        <summary className="text-sm">Not converted</summary>
        <ul className={list}>
          {rows.map((s, i) => (
            // A track selected twice is listed twice; order is stable.
            // biome-ignore lint/suspicious/noArrayIndexKey: see above
            <li key={i}>
              {s.label}: {s.reason}
            </li>
          ))}
        </ul>
      </details>
    </Section>
  );
}

function Section({
  title,
  tone,
  children,
}: {
  title: string;
  tone?: "danger" | "warning";
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

function Facts({ facts: rows }: { facts: ConvertFact[] }) {
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
