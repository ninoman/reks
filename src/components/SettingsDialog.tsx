/**
 * Settings (phase 1 task 13), one tab per group in a sidebar: Library
 * (library folders, cache folder, library backups), Playback (output device, sample rate,
 * buffer, headphone cue: `OutputSettings`), Display (theme, key notation),
 * Decks (the waveform style of the player deck and the Mix view, what SYNC
 * aligns), Mixer (the EQ layout and which mixer controls the Mix view shows).
 * Display, Decks and Mixer live in `settings/`, their choices as picture
 * tiles (`ChoiceTiles`). Values live in Rust (`useSettings`); the library
 * window's own state (columns, panels, volume) is saved without asking.
 */

import { type ReactNode, type RefObject, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { pickFolder } from "../lib/ipc";
import { useScan } from "../store/scan";
import { type SettingsTab, useSettings } from "../store/settings";
import { ErrorMessage } from "./ErrorMessage";
import { OutputSettings } from "./OutputSettings";
import { changeLibraryFolder, findMissingFiles } from "./relocateActions";
import { BackupSettings } from "./settings/BackupSettings";
import { DeckSettings } from "./settings/DeckSettings";
import { DisplaySettings } from "./settings/DisplaySettings";
import { MixerSettings } from "./settings/MixerSettings";
import { settingItemClass as item } from "./settings/SettingItem";
import { TagSettings } from "./settings/TagSettings";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/Tabs";

const itemTitle = "m-0 text-sm font-semibold";
const hint = "mt-0.5 mb-0 text-xs text-fg-muted";
const path = "font-mono text-xs [overflow-wrap:anywhere] select-text";
const row = "mt-3 flex items-center gap-2";

/** Last scan as a short local date and time, e.g. "19 Oct 2026, 20:48". */
function scanDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

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
        // Fits the tallest tab (Mixer's tiles) without scrolling; shorter windows scroll.
        className="h-[648px]"
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
          {settings !== null && <BackupSettings keep={settings.backupKeep} />}
          {settings !== null && <TagSettings on={settings.tagsInComments} />}
        </Panel>
        {settings !== null && (
          <>
            <Panel value="playback">
              <OutputSettings itemClassName={item} />
            </Panel>
            <Panel value="display">
              <DisplaySettings theme={settings.theme} notation={settings.keyNotation} />
            </Panel>
            <Panel value="decks">
              <DeckSettings waveform={settings.deckWaveform} />
            </Panel>
            <Panel value="mixer">
              <MixerSettings
                eq={settings.mixerEq}
                view={settings.mixerView}
                filters={settings.mixerFilters}
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
    const dir = await pickFolder("Add music folder");
    if (dir) await addRoot(dir);
  }

  async function rescan() {
    await useScan.getState().scan([]);
    await loadRoots();
  }

  return (
    <div className={item}>
      <h5 className={itemTitle}>Library folders</h5>
      <p className={hint}>
        Scanned folders are listed here. Removing one keeps its tracks in the library. If a folder
        moved, “Change folder…” points it and its tracks at the new place; “Find missing files…”
        searches any folder for tracks the library lost.
      </p>
      {roots.length > 0 && (
        <Card
          variant="surface"
          className="mt-2.5 rounded-md [&_li]:grid [&_li]:grid-cols-[minmax(0,1fr)_auto] [&_li]:items-center [&_li]:gap-3 [&_li]:py-1.5 [&_li]:pr-1.5 [&_li]:pl-2.5"
          asChild
        >
          <ul className="m-0 list-none p-0">
            {roots.map((r) => (
              <li key={r.id}>
                <span className="flex min-w-0 flex-col">
                  <span className={path} title={r.path}>
                    {r.path}
                  </span>
                  <span className="text-xs whitespace-nowrap text-fg-muted">
                    {r.lastScan === null ? "Never scanned" : `Scanned ${scanDate(r.lastScan)}`}
                  </span>
                </span>
                <span className="flex gap-1">
                  <Button
                    variant="quiet"
                    size="sm"
                    aria-label={`Change folder for ${r.path}`}
                    disabled={scanning}
                    onClick={() => void changeLibraryFolder(r)}
                  >
                    Change folder…
                  </Button>
                  <Button
                    variant="quiet"
                    size="sm"
                    aria-label={`Remove ${r.path}`}
                    disabled={scanning}
                    onClick={() => void removeRoot(r.id)}
                  >
                    Remove
                  </Button>
                </span>
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
        <Button disabled={scanning} onClick={() => void findMissingFiles({ kind: "allMissing" })}>
          Find missing files…
        </Button>
      </div>
    </div>
  );
}

function CacheFolder({ cacheDir }: { cacheDir: string }) {
  const moving = useSettings((s) => s.moving);
  const scanning = useScan((s) => s.scanning);
  const [target, setTarget] = useState<string | null>(null);

  async function pick() {
    const dir = await pickFolder("Move the cache to");
    if (dir) setTarget(dir);
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
