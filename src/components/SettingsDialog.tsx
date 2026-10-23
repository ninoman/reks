/**
 * Settings (phase 1 task 13), one tab per group in a sidebar: Library
 * (library folders, cache folder), Playback (output device, sample rate,
 * buffer, headphone cue: `OutputSettings`), Display (theme, key notation),
 * Decks (the waveform style of the player deck and the Mix view, what SYNC
 * aligns), Mixer (which mixer controls the Mix view shows). Values live in Rust (`useSettings`); the library window's own
 * state (columns, panels, volume) is saved without asking.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { type ReactNode, type RefObject, useRef, useState } from "react";

import { cn } from "../lib/cn";

import type {
  DeckWaveform,
  KeyNotation,
  MixerEq,
  MixerFilters,
  MixerView,
  SyncAlign,
  Theme,
} from "../lib/ipc";
import { useScan } from "../store/scan";
import { type SettingsTab, useSettings } from "../store/settings";
import { ErrorMessage } from "./ErrorMessage";
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
import { Swatch } from "./ui/Swatch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/Tabs";

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

/** Each theme with its canvas and accent colours (tokens.css `--color-theme-*`). */
const THEME_CHOICES: { id: Theme; label: string; canvas: string; accent: string }[] = [
  {
    id: "graphite",
    label: "Graphite",
    canvas: "bg-theme-graphite",
    accent: "bg-theme-graphite-accent",
  },
  { id: "paper", label: "Paper", canvas: "bg-theme-paper", accent: "bg-theme-paper-accent" },
  {
    id: "midnight",
    label: "Midnight",
    canvas: "bg-theme-midnight",
    accent: "bg-theme-midnight-accent",
  },
  { id: "dusk", label: "Dusk", canvas: "bg-theme-dusk", accent: "bg-theme-dusk-accent" },
  { id: "booth", label: "Booth", canvas: "bg-theme-booth", accent: "bg-theme-booth-accent" },
];

const SYNC_ALIGNS: { id: SyncAlign; label: string }[] = [
  { id: "bar", label: "Bar" },
  { id: "beat", label: "Beat" },
];

const MIXER_EQS: { id: MixerEq; label: string; hint: string }[] = [
  { id: "fourBand", label: "4-band", hint: "filters X and Y" },
  { id: "threeBand", label: "3-band", hint: "FX per channel" },
];

const MIXER_VIEWS: { id: MixerView; label: string; hint: string }[] = [
  { id: "simple", label: "Simple", hint: "no crossfader or assigns" },
  { id: "advanced", label: "Advanced", hint: "everything" },
];

const MIXER_FILTERS: { id: MixerFilters; label: string }[] = [
  { id: "show", label: "Show" },
  { id: "hide", label: "Hide" },
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
  const bodyRef = useRef<HTMLDivElement>(null);
  const startTab = useSettings.getState().dialogTab;
  const close = () => useSettings.getState().setDialogOpen(false);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        size="lg"
        className="h-[520px]"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          // Opened on a tab (not the first): its first control, else Close.
          const first =
            startTab === "library"
              ? null
              : bodyRef.current?.querySelector<HTMLElement>(
                  '[role="tabpanel"]:not([hidden]) :is(button, [role="radio"]):not(:disabled)',
                );
          (first ?? closeRef.current)?.focus();
        }}
      >
        <DialogHeader className="border-b border-border pt-5 pb-4">
          <DialogTitle className="text-lg">Settings</DialogTitle>
        </DialogHeader>
        <SettingsBody startTab={startTab} bodyRef={bodyRef} />
        <DialogFooter className="border-t border-border pt-4">
          <Button variant="primary" ref={closeRef} onClick={close}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

type Tab = SettingsTab;

const TABS: { id: Tab; label: string }[] = [
  { id: "library", label: "Library" },
  { id: "playback", label: "Playback" },
  { id: "display", label: "Display" },
  { id: "decks", label: "Decks" },
  { id: "mixer", label: "Mixer" },
];

function SettingsBody({
  startTab,
  bodyRef,
}: {
  startTab: Tab;
  bodyRef: RefObject<HTMLDivElement | null>;
}) {
  const settings = useSettings((s) => s.settings);
  const error = useSettings((s) => s.error);
  const [tab, setTab] = useState<Tab>(startTab);
  return (
    <Tabs
      orientation="vertical"
      value={tab}
      onValueChange={(v) => setTab(v as Tab)}
      className="flex min-h-0 flex-1"
    >
      <TabsList aria-label="Settings" className="w-36 shrink-0 border-r border-border p-2">
        {TABS.map((t) => (
          <TabsTrigger
            key={t.id}
            value={t.id}
            // Everything but the library folders needs the stored settings.
            disabled={t.id !== "library" && settings === null}
          >
            {t.label}
          </TabsTrigger>
        ))}
      </TabsList>
      <DialogBody ref={bodyRef} className="flex-1 py-4">
        {error !== null && <ErrorMessage as="p" className="m-0" message={error} />}
        <Panel value="library">
          <LibraryFolders />
          {settings !== null && <CacheFolder cacheDir={settings.cacheDir} />}
        </Panel>
        {settings !== null && (
          <>
            <Panel value="playback">
              <OutputSettings itemClassName={item} />
            </Panel>
            <Panel value="display">
              <ThemeChoice theme={settings.theme} />
              <KeyNotationChoice notation={settings.keyNotation} />
            </Panel>
            <Panel value="decks">
              <DeckWaveformChoice waveform={settings.deckWaveform} />
              <SyncAlignChoice align={settings.syncAlign} />
            </Panel>
            <Panel value="mixer">
              <MixerEqChoice eq={settings.mixerEq} />
              <MixerViewChoice view={settings.mixerView} />
              <MixerFiltersChoice
                filters={settings.mixerFilters}
                eq={settings.mixerEq}
                view={settings.mixerView}
              />
            </Panel>
          </>
        )}
      </DialogBody>
    </Tabs>
  );
}

/** One tab's settings in a card, split by thin rules. */
function Panel({ value, children }: { value: Tab; children: ReactNode }) {
  return (
    <TabsContent value={value}>
      <Card>{children}</Card>
    </TabsContent>
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

function ThemeChoice({ theme }: { theme: Theme }) {
  return (
    <div className={item}>
      <h5 className={itemTitle}>Theme</h5>
      <p className={hint}>
        Paper is light; Booth is black with the most contrast, for dark rooms, and is the default.
      </p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="Theme"
        value={theme}
        onValueChange={(v) => void useSettings.getState().update({ theme: v as Theme })}
      >
        {THEME_CHOICES.map((t) => (
          <Segment key={t.id} value={t.id}>
            <span className="inline-flex overflow-hidden rounded-[2px] ring-1 ring-border-strong">
              <Swatch size="sm" className={cn(t.canvas, "rounded-none")} />
              <Swatch size="sm" className={cn(t.accent, "rounded-none")} />
            </span>
            {t.label}
          </Segment>
        ))}
      </SegmentedControl>
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

function MixerEqChoice({ eq }: { eq: MixerEq }) {
  return (
    <div className={item}>
      <h5 className={itemTitle}>EQ</h5>
      <p className={hint}>
        4-band has HI, HM, LM and LO and the two filters; 3-band has HI, MID and LOW and an FX knob
        on every channel (filter, echo or reverb). Each keeps its own knob positions. 4-band is the
        default.
      </p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="EQ"
        value={eq}
        onValueChange={(v) => void useSettings.getState().update({ mixerEq: v as MixerEq })}
      >
        {MIXER_EQS.map((m) => (
          <Segment key={m.id} value={m.id}>
            {m.label}
            <span className="font-normal text-fg-subtle">{m.hint}</span>
          </Segment>
        ))}
      </SegmentedControl>
    </div>
  );
}

function MixerViewChoice({ view }: { view: MixerView }) {
  return (
    <div className={item}>
      <h5 className={itemTitle}>Mixer view</h5>
      <p className={hint}>
        Simple hides the crossfader and the assign switches; Advanced shows every control. For both
        EQ layouts. The sound follows what you see. Simple is the default.
      </p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="Mixer view"
        value={view}
        onValueChange={(v) => void useSettings.getState().update({ mixerView: v as MixerView })}
      >
        {MIXER_VIEWS.map((m) => (
          <Segment key={m.id} value={m.id}>
            {m.label}
            <span className="font-normal text-fg-subtle">{m.hint}</span>
          </Segment>
        ))}
      </SegmentedControl>
    </div>
  );
}

function MixerFiltersChoice({
  filters,
  eq,
  view,
}: {
  filters: MixerFilters;
  eq: MixerEq;
  view: MixerView;
}) {
  const threeBand = eq === "threeBand";
  const advanced = view === "advanced";
  return (
    <div className={item}>
      <h5 className={itemTitle}>Filters</h5>
      <p className={hint}>
        {threeBand
          ? "The 3-band EQ has no filters X and Y: each channel's FX knob is its filter."
          : advanced
            ? "The Advanced view always shows both filters."
            : "Whether the Simple view shows the two filters. Hidden filters are off. Show is the default."}
      </p>
      <SegmentedControl
        className="mt-2.5"
        aria-label="Filters"
        disabled={threeBand || advanced}
        value={filters}
        onValueChange={(v) =>
          void useSettings.getState().update({ mixerFilters: v as MixerFilters })
        }
      >
        {MIXER_FILTERS.map((f) => (
          <Segment key={f.id} value={f.id}>
            {f.label}
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
