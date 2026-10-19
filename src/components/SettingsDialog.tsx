/**
 * Settings (phase 1 task 13) in one view, grouped: Folders (library folders,
 * cache folder), Playback (output device), Display (key notation). Values
 * live in Rust (`useSettings`); the library window's own state (columns,
 * panels, volume) is saved without asking.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { type ReactNode, useEffect, useRef, useState } from "react";

import type { KeyNotation } from "../lib/ipc";
import { usePlayer } from "../store/player";
import { useScan } from "../store/scan";
import { useSettings } from "../store/settings";

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
  useEffect(() => closeRef.current?.focus(), []);
  const close = () => useSettings.getState().setDialogOpen(false);
  return (
    <div className="dialog-backdrop">
      <div
        className="dialog wizard settings"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          }
        }}
      >
        <h3>Settings</h3>
        <SettingsBody />
        <div className="dialog-buttons">
          <button type="button" className="primary" ref={closeRef} onClick={close}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function SettingsBody() {
  const settings = useSettings((s) => s.settings);
  const error = useSettings((s) => s.error);
  return (
    <div className="dialog-body">
      {error !== null && (
        <p className="wizard-error" role="alert">
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
        </>
      )}
    </div>
  );
}

/** A titled card; its items are split by thin rules. */
function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-group" aria-label={title}>
      <h4>{title}</h4>
      <div className="settings-card">{children}</div>
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
    <div className="settings-item">
      <h5>Library folders</h5>
      <p className="hint">
        Scanned folders are listed here. Removing one keeps its tracks in the library.
      </p>
      {roots.length > 0 && (
        <ul className="settings-roots">
          {roots.map((r) => (
            <li key={r.id}>
              <span className="path" title={r.path}>
                {r.path}
              </span>
              <span className="scanned">
                {r.lastScan === null ? "Never scanned" : `Scanned ${scanDate(r.lastScan)}`}
              </span>
              <button
                type="button"
                className="quiet"
                aria-label={`Remove ${r.path}`}
                disabled={scanning}
                onClick={() => void removeRoot(r.id)}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="settings-row">
        <button type="button" disabled={scanning} onClick={() => void add()}>
          Add folder…
        </button>
        <button
          type="button"
          disabled={scanning || roots.length === 0}
          onClick={() => void rescan()}
        >
          {scanning ? "Scanning…" : "Rescan all"}
        </button>
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
    <div className="settings-item">
      <label className="settings-row settings-field">
        <span className="item-title">Output device</span>
        <select value={device ?? ""} onChange={(e) => void choose(e.target.value)}>
          <option value="">System default</option>
          {options.map((d) => (
            <option key={d} value={d}>
              {d}
              {devices.includes(d) ? "" : " (not connected)"}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

function KeyNotationChoice({ notation }: { notation: KeyNotation }) {
  return (
    <div className="settings-item">
      <h5>Key notation</h5>
      <p className="hint">How keys are shown. Key edits accept any notation.</p>
      <div className="settings-radios" role="radiogroup" aria-label="Key notation">
        {NOTATIONS.map((n) => (
          <label key={n.id}>
            <input
              type="radio"
              name="key-notation"
              checked={notation === n.id}
              onChange={() => void useSettings.getState().update({ keyNotation: n.id })}
            />
            {n.label}
            <span className="example">{n.example}</span>
          </label>
        ))}
      </div>
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
    <div className="settings-item">
      <h5>Cache folder</h5>
      <p className="hint">Artwork and waveforms. Move it to an empty or new folder.</p>
      <p className="settings-current path">{cacheDir}</p>
      {target === null ? (
        <div className="settings-row">
          <button type="button" disabled={moving || scanning} onClick={() => void pick()}>
            Move…
          </button>
        </div>
      ) : (
        <div className="settings-confirm">
          <p>
            Move the cache to <span className="path">{target}</span>? The old folder is deleted
            after the copy.
          </p>
          <div className="settings-row">
            <button
              type="button"
              className="primary"
              disabled={moving}
              onClick={() => void move(target)}
            >
              {moving ? "Moving…" : "Move"}
            </button>
            <button type="button" disabled={moving} onClick={() => setTarget(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
