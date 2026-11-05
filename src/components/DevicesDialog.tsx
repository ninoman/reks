/**
 * The Sticks view (phase 2 tasks 1 and 15, D27, D42): the removable volumes
 * the OS reports, with their file system, free space, Eject, Sync and what
 * each is to the library (a known stick, or the identify prompt: "Is this
 * '<name>'?" / "New stick"); then the known sticks and the picked one's
 * profile: "Plays on", the playlist ticks (saved as they change), its
 * status, "frees X MB" per ticked entry and a capacity bar. Sync opens the
 * export's review. The list refreshes every two seconds while open, so a
 * stick shows up when it is plugged in. Whether a file system will do for a
 * given player is the export planner's call; the notes here only flag it.
 * Status and prompts are worded in Rust.
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
  TargetProfile,
} from "../lib/ipc";
import { useAdopt } from "../store/adopt";
import { deviceName, useDevices } from "../store/devices";
import { openExportWizard } from "../store/exportWizard";
import { usePlaylists } from "../store/playlists";
import { coveredIds, mountOf, sortedKnown, useProfiles } from "../store/profiles";
import { ErrorMessage } from "./ErrorMessage";
import { EjectIcon } from "./icons";
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
import { Input } from "./ui/Input";
import { Progress } from "./ui/Progress";
import { Select } from "./ui/Select";

const REFRESH_MS = 2000;

/** "Plays on": the oldest player the stick must work on (the profile's target). */
const TARGETS: { value: TargetProfile; label: string }[] = [
  { value: "Generic", label: "Any player (safest)" },
  { value: "Cdj2000Nxs", label: "CDJ-2000NXS and newer" },
  { value: "Cdj2000Nxs2", label: "CDJ-2000NXS2 and newer" },
  { value: "Cdj3000", label: "CDJ-3000 and newer" },
  { value: "XdjXz", label: "XDJ-XZ" },
];

/** Indent per tree level, as in the sidebar tree. */
const INDENT = 18;

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
          <DialogTitle>Sticks</DialogTitle>
          <DialogDescription>
            Pick a stick and tick its playlists. Ticks are saved as you go; nothing is written until
            you press Export in the review.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <DeviceList />
          <Sticks />
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => useDevices.getState().setDialogOpen(false)}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The plugged-in volumes, refreshed while mounted. */
export function DeviceList() {
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
        <Card asChild>
          <ul aria-label="Plugged in" className="m-0 list-none p-0">
            {devices.map((d) => (
              <DeviceRow key={d.mountPoint} device={d} />
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

function DeviceRow({ device }: { device: DeviceInfo }) {
  const ejecting = useDevices((s) => s.ejecting);
  const known = useProfiles((s) => {
    const match = s.mounted[device.mountPoint]?.match;
    return match?.type === "known" ? match.device : null;
  });
  const name = known?.name ?? deviceName(device);
  const note = fsNote(device.fs);

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
    <li className="flex flex-col gap-2 px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1 select-text">{info}</div>
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
        {known !== null && (
          <Button
            size="sm"
            variant="primary"
            disabled={ejecting !== null}
            aria-label={`Sync ${known.name}`}
            onClick={() => openExportWizard(device.mountPoint)}
          >
            Sync
          </Button>
        )}
      </div>
      <StickIdentity mountPoint={device.mountPoint} />
    </li>
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
            variant="primary"
            className="shrink-0"
            disabled={mountPoint === null}
            title={mountPoint === null ? "Plug this stick in to sync it" : undefined}
            onClick={() => {
              if (mountPoint !== null) openExportWizard(mountPoint);
            }}
          >
            Sync {s.device.name}
          </Button>
        </div>
      </div>

      <Field label="Plays on" hint="The oldest player this stick must work on.">
        <Select
          aria-label="Plays on"
          value={s.device.target}
          onChange={(e) => void useProfiles.getState().setTarget(e.target.value as TargetProfile)}
        >
          {TARGETS.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </Select>
      </Field>

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

      <PlaylistTicks status={s} />

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

/** The library's playlists with the selected stick's ticks; a ticked folder covers what's under it. */
function PlaylistTicks({ status: s }: { status: ProfileStatus }) {
  const nodes = usePlaylists((p) => p.nodes);
  const ticks = useProfiles((p) => p.ticks);
  if (nodes.length === 0) {
    return <p className="m-0 text-sm text-fg-muted">The library has no playlists yet.</p>;
  }
  if (ticks === null) return <Progress aria-label="Reading the stick's playlists" />;
  const covered = coveredIds(nodes, ticks);
  const frees = new Map(s.entries.map((e) => [e.playlistId, e.freesText]));
  return (
    <div className="flex flex-col gap-1">
      <h4 className="m-0 text-xs font-semibold text-fg-muted">Playlists on this stick</h4>
      <Card role="group" aria-label="Playlists on this stick" divided={false} className="py-1">
        {nodes.map((n) => {
          const implied = covered.has(n.id);
          return (
            <label
              key={n.id}
              className="flex h-(--row-h) items-center gap-2 pr-2 text-md"
              style={{ paddingLeft: 8 + n.depth * INDENT }}
              title={
                n.isFolder ? "A folder takes every playlist under it, also later ones" : undefined
              }
            >
              <Checkbox
                checked={implied || ticks.has(n.id)}
                disabled={implied}
                onChange={() => void useProfiles.getState().toggleTick(n.id, nodes)}
              />
              <span className="flex-1 truncate">{n.name}</span>
              {n.smart && (
                <Badge variant="muted" title="Written as a fixed list of its current tracks">
                  Smart
                </Badge>
              )}
              <span className="text-xs text-fg-subtle tabular-nums">
                {frees.get(n.id) ?? (n.isFolder ? "folder" : n.entries)}
              </span>
            </label>
          );
        })}
      </Card>
    </div>
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
