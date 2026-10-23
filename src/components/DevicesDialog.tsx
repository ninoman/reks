/**
 * USB devices (phase 2 task 1) and stick profiles (task 15, D27): the
 * removable volumes the OS reports, with their file system, free space,
 * Eject and what each is to the library (a known stick, or the identify
 * prompt: "Is this '<name>'?" / "New stick"); then the known sticks with
 * their profile status, "frees X MB" per ticked entry and a capacity bar.
 * The list refreshes every two seconds while open, so a stick shows up when
 * it is plugged in. `DeviceList` is also the export wizard's device step
 * (task 10): pass `selected` and `onSelect` to pick one. Whether a file
 * system will do for a given player is the export planner's call; the notes
 * here only flag it. Status and prompts are worded in Rust.
 */
import { type FormEvent, useEffect, useState } from "react";

import { cn } from "../lib/cn";
import { formatBytes } from "../lib/format";
import type {
  Device,
  DeviceInfo,
  DeviceMatch,
  FsKind,
  PlaylistStatus,
  ProfileStatus,
  SyncState,
} from "../lib/ipc";
import { useAdopt } from "../store/adopt";
import { deviceName, useDevices } from "../store/devices";
import { openExportWizard } from "../store/exportWizard";
import { mountOf, sortedKnown, useProfiles } from "../store/profiles";
import { ErrorMessage } from "./ErrorMessage";
import { EjectIcon } from "./icons";
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
import { Input } from "./ui/Input";
import { Progress } from "./ui/Progress";

const REFRESH_MS = 2000;

function fsLabel(fs: FsKind): string {
  switch (fs.kind) {
    case "fat32":
      return "FAT32";
    case "exFat":
      return "exFAT";
    case "hfs":
      return "Mac OS Extended";
    case "other":
      return fs.name.toUpperCase();
  }
}

/** Same reading of the file system as the planner's warnings and errors (D16). */
function fsNote(fs: FsKind): { tone: "warning" | "danger"; text: string } | null {
  switch (fs.kind) {
    case "fat32":
      return null;
    case "exFat":
      return { tone: "warning", text: "exFAT: newer players only. FAT32 works everywhere." };
    case "hfs":
      return { tone: "warning", text: "Mac OS Extended: some players can't read it." };
    case "other":
      return { tone: "danger", text: "Players can't read this file system. Format it as FAT32." };
  }
}

export function DevicesDialog() {
  const open = useDevices((s) => s.dialogOpen);
  if (!open) return null;
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) useDevices.getState().setDialogOpen(false);
      }}
    >
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>USB devices</DialogTitle>
          <DialogDescription>
            Removable drives connected to this computer, and the playlists each of your sticks
            carries.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <DeviceList />
          <Sticks />
        </DialogBody>
        <DialogFooter>
          <Button
            onClick={() => {
              useDevices.getState().setDialogOpen(false);
              openExportWizard();
            }}
          >
            Export to USB…
          </Button>
          <Button variant="primary" onClick={() => useDevices.getState().setDialogOpen(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export interface DeviceListProps {
  /** Mount point of the chosen device; with `onSelect`, rows become a radio group. */
  selected?: string | null;
  onSelect?: (device: DeviceInfo) => void;
}

/** The device list, refreshed while mounted. */
export function DeviceList({ selected = null, onSelect }: DeviceListProps) {
  const devices = useDevices((s) => s.devices);
  const loaded = useDevices((s) => s.loaded);
  const error = useDevices((s) => s.error);
  const notice = useDevices((s) => s.notice);

  useEffect(() => {
    const { refresh } = useDevices.getState();
    void useProfiles.getState().refreshKnown();
    void refresh();
    const timer = window.setInterval(() => void refresh(), REFRESH_MS);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (loaded) void useProfiles.getState().syncMounted(devices);
  }, [devices, loaded]);

  return (
    <div className="flex flex-col gap-3">
      {error !== null && <ErrorMessage as="p" className="m-0" message={error} />}
      {notice !== null && (
        <p className="m-0 text-success" role="status">
          {notice}
        </p>
      )}
      {loaded && devices.length === 0 ? (
        <p className="m-0 text-fg-muted">No USB drive found. Plug one in and it shows up here.</p>
      ) : (
        <Card role={onSelect ? "radiogroup" : "list"} aria-label="USB devices">
          {devices.map((d) => (
            <DeviceRow
              key={d.mountPoint}
              device={d}
              selected={d.mountPoint === selected}
              onSelect={onSelect}
            />
          ))}
        </Card>
      )}
    </div>
  );
}

function DeviceRow({
  device,
  selected,
  onSelect,
}: {
  device: DeviceInfo;
  selected: boolean;
  onSelect?: ((device: DeviceInfo) => void) | undefined;
}) {
  const ejecting = useDevices((s) => s.ejecting);
  const name = deviceName(device);
  const note = fsNote(device.fs);
  const pickable = onSelect !== undefined;

  const info = (
    <>
      <p className="m-0 truncate text-sm font-semibold">{name}</p>
      <p className="m-0 text-xs text-fg-muted tabular-nums">
        {fsLabel(device.fs)} · {formatBytes(device.freeBytes)} free of{" "}
        {formatBytes(device.totalBytes)}
      </p>
      <p className="m-0 truncate font-mono text-xs text-fg-subtle">{device.mountPoint}</p>
      {note !== null && (
        <p
          className={
            note.tone === "danger" ? "m-0 text-xs text-danger" : "m-0 text-xs text-warning"
          }
        >
          {note.text}
        </p>
      )}
    </>
  );

  return (
    <div
      role={pickable ? undefined : "listitem"}
      data-selected={selected || undefined}
      className="flex flex-col gap-2 px-4 py-3 data-[selected]:bg-selected data-[selected]:text-selected-fg"
    >
      <div className="flex items-center gap-3">
        {pickable ? (
          <label className="flex min-w-0 flex-1 items-center gap-3">
            <input
              type="radio"
              name="reks-device"
              aria-label={name}
              checked={selected}
              onChange={() => onSelect(device)}
              className="shrink-0 accent-accent-solid"
            />
            <span className="min-w-0 flex-1">{info}</span>
          </label>
        ) : (
          <div className="min-w-0 flex-1 select-text">{info}</div>
        )}
        <Button
          size="sm"
          variant="outline"
          disabled={ejecting !== null}
          aria-label={`Eject ${name}`}
          onClick={() => void useDevices.getState().eject(device.mountPoint)}
        >
          <EjectIcon />
          {ejecting === device.mountPoint ? "Ejecting…" : "Eject"}
        </Button>
      </div>
      <StickIdentity mountPoint={device.mountPoint} />
    </div>
  );
}

/** What the volume at `mountPoint` is to the library: its profile, or the identify prompt. */
function StickIdentity({ mountPoint }: { mountPoint: string }) {
  const entry = useProfiles((s) => s.mounted[mountPoint]);
  if (entry === undefined) return null;
  if (entry.error !== null) {
    return <ErrorMessage as="p" className="m-0 text-xs" message={entry.error} />;
  }
  const match = entry.match;
  if (match === null) return <p className="m-0 text-xs text-fg-subtle">Identifying…</p>;
  if (match.type === "known") {
    return <p className="m-0 text-xs text-fg-muted">Stick: {match.device.name}</p>;
  }
  return <IdentifyPrompt mountPoint={mountPoint} match={match} />;
}

/**
 * The identify prompts (stick-profiles §4.2): "Is this '<name>'?" when label
 * and size match known sticks (every time when the OS gives no volume ID),
 * otherwise naming a new stick.
 */
export function IdentifyPrompt({
  mountPoint,
  match,
}: {
  mountPoint: string;
  match: Exclude<DeviceMatch, { type: "known" }>;
}) {
  const [naming, setNaming] = useState(match.type !== "ambiguous");
  const [busy, setBusy] = useState(false);
  const suggested = match.type === "ambiguous" ? "" : match.suggestedName;
  const [name, setName] = useState(suggested);
  const profiles = useProfiles.getState();

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
    } finally {
      setBusy(false);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (name.trim() !== "") void run(() => profiles.create(mountPoint, name));
  };
  /** Foreign stick: remember it, then open the adopt dry run. */
  const adopt = () =>
    void run(async () => {
      const device = await profiles.create(mountPoint, name);
      if (device !== null) openAdopt(device, mountPoint);
    });

  return (
    <fieldset
      aria-label="Identify this stick"
      className="m-0 flex min-w-0 flex-col gap-2 rounded-md border border-border bg-surface px-3 py-2 text-fg"
    >
      <p className="m-0 text-sm font-semibold">{match.prompt}</p>
      <p className={cn("m-0 text-xs", match.type === "foreign" ? "text-warning" : "text-fg-muted")}>
        {match.note}
      </p>
      {match.type === "ambiguous" && !naming && (
        <div className="flex flex-wrap gap-2">
          {match.candidates.map((c) => (
            <Button
              key={c.id}
              size="sm"
              variant="primary"
              disabled={busy}
              onClick={() => void run(() => profiles.link(mountPoint, c.id))}
            >
              {match.candidates.length === 1 ? "Yes" : c.name}
            </Button>
          ))}
          <Button size="sm" disabled={busy} onClick={() => setNaming(true)}>
            {match.candidates.length === 1 ? "No, it's another stick" : "None of these"}
          </Button>
        </div>
      )}
      {naming && (
        <form className="flex items-center gap-2" onSubmit={submit}>
          <Input
            size="sm"
            aria-label="Stick name"
            placeholder="Name this stick"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          {match.type === "foreign" && (
            <Button
              size="sm"
              variant="primary"
              disabled={busy || name.trim() === ""}
              className="shrink-0"
              onClick={adopt}
            >
              Adopt…
            </Button>
          )}
          <Button
            size="sm"
            variant={match.type === "foreign" ? "secondary" : "primary"}
            type="submit"
            disabled={busy || name.trim() === ""}
            className="shrink-0"
          >
            New stick
          </Button>
        </form>
      )}
    </fieldset>
  );
}

/** Opens the adopt dialog for the plugged-in stick `device` (closing this one). */
function openAdopt(device: Device, mountPoint: string) {
  const info = useProfiles.getState().mounted[mountPoint]?.info;
  if (info === undefined) return;
  useDevices.getState().setDialogOpen(false);
  useAdopt.getState().open({ deviceId: device.id, deviceName: device.name, info });
}

/** Known sticks and the selected one's profile status. */
function Sticks() {
  const known = useProfiles((s) => s.known);
  const mounted = useProfiles((s) => s.mounted);
  const selectedId = useProfiles((s) => s.selectedId);
  const status = useProfiles((s) => s.status);
  const loading = useProfiles((s) => s.statusLoading);
  const error = useProfiles((s) => s.error);

  useEffect(() => {
    // Reopened after a sync: show what it recorded.
    void useProfiles.getState().loadStatus();
  }, []);

  return (
    <section aria-label="Your sticks" className="flex flex-col gap-2">
      <h3 className="m-0 text-sm font-semibold">Your sticks</h3>
      {error !== null && <ErrorMessage as="p" className="m-0" message={error} />}
      {known.length === 0 ? (
        <p className="m-0 text-fg-muted">
          No sticks saved yet. Plug one in and name it; reks remembers which playlists it carries.
        </p>
      ) : (
        <Card role="radiogroup" aria-label="Your sticks">
          {sortedKnown(known, mounted).map((d) => (
            <StickRow
              key={d.id}
              device={d}
              mountPoint={mountOf(mounted, d.id)}
              selected={d.id === selectedId}
            />
          ))}
        </Card>
      )}
      {selectedId !== null &&
        (status !== null && status.device.id === selectedId ? (
          <StatusView status={status} mountPoint={mountOf(mounted, selectedId)} />
        ) : (
          loading && <Progress aria-label="Reading the profile" />
        ))}
    </section>
  );
}

function StickRow({
  device,
  mountPoint,
  selected,
}: {
  device: Device;
  mountPoint: string | null;
  selected: boolean;
}) {
  return (
    <label
      data-selected={selected || undefined}
      className="flex items-center gap-3 px-4 py-2 data-[selected]:bg-selected data-[selected]:text-selected-fg"
    >
      <input
        type="radio"
        name="reks-stick"
        aria-label={device.name}
        checked={selected}
        onChange={() => useProfiles.getState().select(device.id)}
        className="shrink-0 accent-accent-solid"
      />
      <span className="min-w-0 flex-1 truncate text-sm font-semibold">{device.name}</span>
      <span className="shrink-0 text-xs text-fg-muted tabular-nums">
        {mountPoint !== null ? "Plugged in" : formatBytes(device.sizeBytes)}
      </span>
    </label>
  );
}

const STATE_TONE: Record<SyncState["state"], string> = {
  inSync: "text-fg-muted",
  new: "text-success",
  changed: "text-warning",
  willRemove: "text-danger",
};

/** One stick's profile: summary, capacity, ticked entries with "frees", per-playlist status. */
export function StatusView({
  status: s,
  mountPoint,
}: {
  status: ProfileStatus;
  mountPoint: string | null;
}) {
  return (
    <Card className="flex flex-col gap-3 px-4 py-3 select-text" divided={false}>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="m-0 text-sm font-semibold" role="status">
            {s.device.name}: {s.summary}
          </p>
          <p className="m-0 text-xs text-fg-muted">{s.sourceText}</p>
        </div>
        <div className="flex shrink-0 gap-2">
          {mountPoint !== null && (
            <Button
              size="sm"
              variant="ghost"
              className="shrink-0"
              title="Keep this stick's playlists as library playlists"
              onClick={() => openAdopt(s.device, mountPoint)}
            >
              Adopt playlists…
            </Button>
          )}
          <Button
            size="sm"
            className="shrink-0"
            disabled={mountPoint === null}
            title={mountPoint === null ? "Plug this stick in to change or sync it" : undefined}
            onClick={() => {
              if (mountPoint === null) return;
              useDevices.getState().setDialogOpen(false);
              openExportWizard(mountPoint);
            }}
          >
            Edit and sync…
          </Button>
        </div>
      </div>

      <div className="flex flex-col gap-1">
        <Progress
          max={Math.max(s.capacityBytes, 1)}
          value={Math.min(s.plannedBytes, s.capacityBytes)}
          aria-label="Planned space on the stick"
        />
        <p
          className={cn(
            "m-0 text-xs tabular-nums",
            s.overCapacity ? "text-danger" : "text-fg-muted",
          )}
        >
          {s.capacityText}
        </p>
      </div>

      {s.notes.length > 0 && (
        <ul className="m-0 list-disc pl-4.5 text-xs text-warning" aria-label="Notes">
          {s.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}

      {s.entries.length > 0 && (
        <div className="flex flex-col gap-1">
          <h4 className="m-0 text-xs font-semibold text-fg-muted">Ticked</h4>
          <ul className="m-0 flex list-none flex-col gap-1 p-0" aria-label="Ticked playlists">
            {s.entries.map((e) => (
              <li key={e.playlistId} className="flex items-baseline justify-between gap-4">
                <span className="min-w-0 truncate text-sm">
                  {e.pathText}
                  {e.isFolder ? "/" : ""}
                </span>
                <span className="shrink-0 text-xs text-fg-muted tabular-nums">{e.freesText}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {s.playlists.length > 0 && (
        <div className="flex flex-col gap-1">
          <h4 className="m-0 text-xs font-semibold text-fg-muted">On the stick after a sync</h4>
          <ul className="m-0 flex list-none flex-col gap-1 p-0" aria-label="Playlist status">
            {s.playlists.map((p, i) => (
              // Two stick playlists can share a path (same names); order is stable.
              // biome-ignore lint/suspicious/noArrayIndexKey: see above
              <PlaylistRow key={i} row={p} />
            ))}
          </ul>
        </div>
      )}
    </Card>
  );
}

function PlaylistRow({ row }: { row: PlaylistStatus }) {
  return (
    <li className="flex items-baseline justify-between gap-4">
      <span
        className={cn(
          "min-w-0 truncate text-sm",
          row.state.state === "willRemove" && "text-fg-muted line-through",
        )}
      >
        {row.pathText}
        <span className="text-xs text-fg-subtle tabular-nums"> · {row.tracks}</span>
      </span>
      <span className={cn("shrink-0 text-xs", STATE_TONE[row.state.state])}>{row.text}</span>
    </li>
  );
}
