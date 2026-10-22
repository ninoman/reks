/**
 * Settings (phase 1 task 13) in one view, grouped: Folders (library folders,
 * cache folder), Playback (output device, sample rate, buffer, headphone
 * cue: `OutputSettings`), Display (key notation), Decks
 * (the waveform style of the player deck and the Mix view, what SYNC
 * aligns). Values
 * live in Rust (`useSettings`); the library window's own state (columns,
 * panels, volume) is saved without asking.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { type ReactNode, useRef, useState } from "react";

import { cn } from "../lib/cn";

import type { DeckWaveform, KeyNotation, SyncAlign } from "../lib/ipc";
import { useScan } from "../store/scan";
import { useSettings } from "../store/settings";
import { OutputSettings } from "./OutputSettings";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";

const item = "px-4 py-3.5";
const itemTitle = "m-0 text-sm font-semibold";
const hint = "mt-0.5 mb-0 text-xs text-fg-muted";
const path = "font-mono text-xs [overflow-wrap:anywhere] select-text";
const row = "mt-3 flex items-center gap-2";

/** Last scan as a short local date and time, e.g. "19 Oct 2026, 20:48". */
function scanDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

const NOTATIONS: { id: KeyNotation; label: string; example: string }[] = [
  { id: "classical", label: "Classical", example: "Am, F#, Db" },
  { id: "camelot", label: "Camelot", example: "8A, 2B, 3B" },
  { id: "openKey", label: "Open Key", example: "1m, 7d, 8d" },
];

const SYNC_ALIGNS: { id: SyncAlign; label: string }[] = [
  { id: "bar", label: "Bar" },
  { id: "beat", label: "Beat" },
];

const WAVEFORMS: { id: DeckWaveform; label: string; hint: string }[] = [
  { id: "rgb", label: "RGB", hint: "colour by band" },
  { id: "threeBand", label: "3-band", hint: "lows, mids, highs" },
  { id: "blue", label: "Blue", hint: "classic" },
];

export function SettingsDialog() {
  const isOpen = useSettings((s) => s.dialogOpen);
  if (!isOpen) return null;
  return <SettingsWindow />;
}

function SettingsWindow() {
  const closeRef = useRef<HTMLButtonElement>(null);
  const close = () => useSettings.getState().setDialogOpen(false);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        size="md"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          closeRef.current?.focus();
        }}
      >
        <DialogHeader className="pt-5">
          <DialogTitle className="text-lg">Settings</DialogTitle>
        </DialogHeader>
        <SettingsBody />
        <DialogFooter className="pt-5">
          <Button variant="primary" ref={closeRef} onClick={close}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SettingsBody() {
  const settings = useSettings((s) => s.settings);
  const error = useSettings((s) => s.error);
  return (
    <DialogBody className="gap-5">
      {error !== null && (
        <p className="text-danger" role="alert">
          {error}
        </p>
      )}
      <Group title="Folders">
        <LibraryFolders />
        {settings !== null && <CacheFolder cacheDir={settings.cacheDir} />}
      </Group>
      {settings !== null && (
        <>
          <Group title="Playback">
            <OutputSettings itemClassName={item} />
          </Group>
          <Group title="Display">
            <KeyNotationChoice notation={settings.keyNotation} />
          </Group>
          <Group title="Decks">
            <DeckWaveformChoice waveform={settings.deckWaveform} />
            <SyncAlignChoice align={settings.syncAlign} />
          </Group>
        </>
      )}
    </DialogBody>
  );
}

/** A titled card; its items are split by thin rules. */
function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title}>
      <h4 className="mt-0 mb-2 ml-0.5 text-2xs font-semibold tracking-caption text-fg-muted uppercase">
        {title}
      </h4>
      <Card>{children}</Card>
    </section>
  );
}

function LibraryFolders() {
  const roots = useSettings((s) => s.roots);
  const scanning = useScan((s) => s.scanning);
  const { addRoot, removeRoot, loadRoots } = useSettings.getState();

  async function add() {
    const dir = await open({ directory: true, title: "Add music folder" });
    if (typeof dir === "string") await addRoot(dir);
  }

  async function rescan() {
    await useScan.getState().scan([]);
    await loadRoots();
  }

  return (
    <div className={item}>
      <h5 className={itemTitle}>Library folders</h5>
      <p className={hint}>
        Scanned folders are listed here. Removing one keeps its tracks in the library.
      </p>
      {roots.length > 0 && (
        <Card
          variant="surface"
          className="mt-2.5 rounded-md [&_li]:grid [&_li]:grid-cols-[1fr_auto_auto] [&_li]:items-center [&_li]:gap-3 [&_li]:py-1.5 [&_li]:pr-1.5 [&_li]:pl-2.5"
          asChild
        >
          <ul className="m-0 list-none p-0">
            {roots.map((r) => (
              <li key={r.id}>
                <span className={path} title={r.path}>
                  {r.path}
                </span>
                <span className="text-xs whitespace-nowrap text-fg-muted">
                  {r.lastScan === null ? "Never scanned" : `Scanned ${scanDate(r.lastScan)}`}
                </span>
                <Button
                  variant="quiet"
                  size="sm"
                  aria-label={`Remove ${r.path}`}
                  disabled={scanning}
                  onClick={() => void removeRoot(r.id)}
                >
                  Remove
                </Button>
              </li>
            ))}
          </ul>
        </Card>
      )}
      <div className={row}>
        <Button disabled={scanning} onClick={() => void add()}>
          Add folder…
        </Button>
        <Button disabled={scanning || roots.length === 0} onClick={() => void rescan()}>
          {scanning ? "Scanning…" : "Rescan all"}
        </Button>
      </div>
    </div>
  );
}

function KeyNotationChoice({ notation }: { notation: KeyNotation }) {
  return (
    <div className={item}>
      <h5 className={itemTitle}>Key notation</h5>
      <p className={hint}>How keys are shown. Key edits accept any notation.</p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="Key notation"
        value={notation}
        onValueChange={(v) => void useSettings.getState().update({ keyNotation: v as KeyNotation })}
      >
        {NOTATIONS.map((n) => (
          <Segment key={n.id} value={n.id}>
            {n.label}
            <span className="font-normal text-fg-subtle">{n.example}</span>
          </Segment>
        ))}
      </SegmentedControl>
    </div>
  );
}

function DeckWaveformChoice({ waveform }: { waveform: DeckWaveform }) {
  return (
    <div className={item}>
      <h5 className={itemTitle}>Waveform</h5>
      <p className={hint}>
        How the player deck and the Mix view draw waveforms and overviews. RGB is the default.
      </p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="Deck waveform"
        value={waveform}
        onValueChange={(v) =>
          void useSettings.getState().update({ deckWaveform: v as DeckWaveform })
        }
      >
        {WAVEFORMS.map((w) => (
          <Segment key={w.id} value={w.id}>
            {w.label}
            <span className="font-normal text-fg-subtle">{w.hint}</span>
          </Segment>
        ))}
      </SegmentedControl>
    </div>
  );
}

function SyncAlignChoice({ align }: { align: SyncAlign }) {
  return (
    <div className={item}>
      <h5 className={itemTitle}>Sync aligns</h5>
      <p className={hint}>
        Bar: SYNC lines up the downbeats with the master as well as the beat. Bar is the default.
      </p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="Sync aligns"
        value={align}
        onValueChange={(v) => void useSettings.getState().update({ syncAlign: v as SyncAlign })}
      >
        {SYNC_ALIGNS.map((a) => (
          <Segment key={a.id} value={a.id}>
            {a.label}
          </Segment>
        ))}
      </SegmentedControl>
    </div>
  );
}

function CacheFolder({ cacheDir }: { cacheDir: string }) {
  const moving = useSettings((s) => s.moving);
  const scanning = useScan((s) => s.scanning);
  const [target, setTarget] = useState<string | null>(null);

  async function pick() {
    const dir = await open({ directory: true, title: "Move the cache to" });
    if (typeof dir === "string") setTarget(dir);
  }

  async function move(to: string) {
    await useSettings.getState().moveCache(to);
    setTarget(null);
  }

  return (
    <div className={item}>
      <h5 className={itemTitle}>Cache folder</h5>
      <p className={hint}>Artwork and waveforms. Move it to an empty or new folder.</p>
      <p
        className={cn(
          path,
          "mt-2.5 rounded-md border border-border bg-surface px-2.5 py-1.5 text-fg",
        )}
      >
        {cacheDir}
      </p>
      {target === null ? (
        <div className={row}>
          <Button disabled={moving || scanning} onClick={() => void pick()}>
            Move…
          </Button>
        </div>
      ) : (
        <div className="mt-3 rounded-md border border-border bg-surface px-3 py-2.5">
          <p className="m-0">
            Move the cache to <span className={path}>{target}</span>? The old folder is deleted
            after the copy.
          </p>
          <div className={row}>
            <Button variant="primary" disabled={moving} onClick={() => void move(target)}>
              {moving ? "Moving…" : "Move"}
            </Button>
            <Button disabled={moving} onClick={() => setTarget(null)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
