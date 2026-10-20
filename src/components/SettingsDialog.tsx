/**
 * Settings (phase 1 task 13) in one view, grouped: Folders (library folders,
 * cache folder), Playback (output device), Display (key notation),
 * Experimental (the OneLibrary key, phase 4 task 8; write-only: Rust only
 * says whether one is stored). Values
 * live in Rust (`useSettings`); the library window's own state (columns,
 * panels, volume) is saved without asking.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { type ReactNode, useEffect, useRef, useState } from "react";

import { cn } from "../lib/cn";

import type { KeyNotation } from "../lib/ipc";
import { usePlayer } from "../store/player";
import { useScan } from "../store/scan";
import { useSettings } from "../store/settings";
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
import { Field } from "./ui/Field";
import { Input } from "./ui/Input";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Select } from "./ui/Select";

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
            <OutputDevice device={settings.audioDevice} />
          </Group>
          <Group title="Display">
            <KeyNotationChoice notation={settings.keyNotation} />
          </Group>
          <Group title="Experimental">
            <OneLibraryKey keySet={settings.onelibraryKeySet} />
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

function OutputDevice({ device }: { device: string | null }) {
  const devices = usePlayer((s) => s.devices);
  useEffect(() => {
    void usePlayer.getState().refreshDevices();
  }, []);

  async function choose(value: string) {
    await usePlayer.getState().setDevice(value === "" ? null : value);
    await useSettings.getState().refresh();
  }

  // A chosen device that is not connected stays listed so the choice shows.
  const options = device !== null && !devices.includes(device) ? [device, ...devices] : devices;
  return (
    <div className={item}>
      <Field label="Output device" orientation="horizontal">
        <Select
          className="min-w-55"
          value={device ?? ""}
          onChange={(e) => void choose(e.target.value)}
        >
          <option value="">System default</option>
          {options.map((d) => (
            <option key={d} value={d}>
              {d}
              {devices.includes(d) ? "" : " (not connected)"}
            </option>
          ))}
        </Select>
      </Field>
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

function OneLibraryKey({ keySet }: { keySet: boolean }) {
  const [draft, setDraft] = useState("");
  const update = (onelibraryKey: string | null) => useSettings.getState().update({ onelibraryKey });

  async function save() {
    await update(draft.trim());
    setDraft("");
  }

  return (
    <div className={item}>
      <h5 className={itemTitle}>OneLibrary key</h5>
      <p className={hint}>
        The OneLibrary export option also writes exportLibrary.db, the library newer players read.
        It is experimental: no player has been tested with reks's file, and AlphaTheta does not
        support it.
      </p>
      <p className={hint}>
        rekordbox encrypts exportLibrary.db with a fixed passphrase. reks does not include it;
        community projects that document the format have published it. Paste it here to use the
        option. reks keeps it in its library database on this computer and never shows or logs it.
        Without a stored key, reks reads REKS_ONELIBRARY_KEY from the environment.
      </p>
      <p className="mt-2.5 mb-0 text-sm" role="status">
        {keySet ? "A key is stored." : "No key stored."}
      </p>
      <form
        className={row}
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.trim() !== "") void save();
        }}
      >
        <Input
          type="password"
          aria-label="OneLibrary key"
          placeholder={keySet ? "Paste a new key to replace it" : "Paste the key"}
          autoComplete="off"
          spellCheck={false}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="submit" disabled={draft.trim() === ""}>
          Save
        </Button>
        {keySet && (
          <Button type="button" variant="quiet" onClick={() => void update(null)}>
            Remove
          </Button>
        )}
      </form>
    </div>
  );
}
