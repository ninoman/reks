/**
 * The Sticks view (phase 2 tasks 1 and 15, D27, D42): one list of sticks,
 * plugged-in known sticks first, then drives reks doesn't know yet (with the
 * identify prompt: "Is this '<name>'?" / "New stick"), then known sticks
 * that are not plugged in. The picked stick expands to its file system,
 * space, "Plays on", capacity and sync status; its playlist ticks (saved as
 * they change) sit in the right column. Sync in the footer opens the
 * export. The list refreshes every two seconds while open, so a stick shows
 * up when it is plugged in. Whether a file system will do for a given
 * player is the export planner's call; the notes here only flag it. Status
 * and prompts are worded in Rust.
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
import { mountOf, sortedKnown, useProfiles } from "../store/profiles";
import { ErrorMessage } from "./ErrorMessage";
import { EjectIcon } from "./icons";
import { PlaylistTicks } from "./PlaylistTicks";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import {
  DialogBody,
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

/** The Sticks view's header, body and footer; `SticksDialog` hosts it. */
export function SticksPanel() {
  return (
    <>
      <DialogHeader>
        <DialogTitle>Sticks</DialogTitle>
        <DialogDescription>
          Pick a stick and tick its playlists. Ticks are saved as you go; nothing is written until
          you press Export in the review.
        </DialogDescription>
      </DialogHeader>
      <DialogBody className="grid min-h-0 flex-1 grid-cols-1 gap-4 overflow-hidden md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <div className="flex min-h-0 flex-col gap-1">
          <h4 className="m-0 shrink-0 text-xs font-semibold text-fg-muted">Sticks</h4>
          <div className="scroll-pane flex min-h-0 flex-col gap-3">
            <StickList />
          </div>
        </div>
        <StickPlaylists />
      </DialogBody>
      <DialogFooter>
        <Button onClick={() => useDevices.getState().setDialogOpen(false)}>Close</Button>
        <SyncButton />
      </DialogFooter>
    </>
  );
}

/** Sync for the picked stick; it must be plugged in. */
function SyncButton() {
  const selectedId = useProfiles((s) => s.selectedId);
  const known = useProfiles((s) => s.known);
  const mounted = useProfiles((s) => s.mounted);
  const device = known.find((d) => d.id === selectedId);
  if (device === undefined) return null;
  const mountPoint = mountOf(mounted, device.id);
  return (
    <Button
      variant="primary"
      disabled={mountPoint === null}
      title={mountPoint === null ? "Plug this stick in to sync it" : undefined}
      onClick={() => {
        if (mountPoint !== null) openExportWizard(mountPoint);
      }}
    >
      Sync {device.name}
    </Button>
  );
}

/** Every stick in one list, refreshed while mounted; the picked one expanded. */
function StickList() {
  const devices = useDevices((s) => s.devices);
  const loaded = useDevices((s) => s.loaded);
  const devicesError = useDevices((s) => s.error);
  const notice = useDevices((s) => s.notice);
  const known = useProfiles((s) => s.known);
  const mounted = useProfiles((s) => s.mounted);
  const selectedId = useProfiles((s) => s.selectedId);
  const error = useProfiles((s) => s.error);

  useEffect(() => {
    const { refresh } = useDevices.getState();
    void useProfiles.getState().refreshKnown();
    void refresh();
    // Reopened after a sync: show what it recorded.
    void useProfiles.getState().loadStatus();
    const timer = window.setInterval(() => void refresh(), REFRESH_MS);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (loaded) void useProfiles.getState().syncMounted(devices);
  }, [devices, loaded]);

  const sticks = sortedKnown(known, mounted);
  const pluggedIn = sticks.filter((d) => mountOf(mounted, d.id) !== null);
  const away = sticks.filter((d) => mountOf(mounted, d.id) === null);
  const unknown = devices.filter((d) => mounted[d.mountPoint]?.match?.type !== "known");

  return (
    <div className="flex flex-col gap-3">
      {devicesError !== null && <ErrorMessage as="p" className="m-0" message={devicesError} />}
      {error !== null && <ErrorMessage as="p" className="m-0" message={error} />}
      {notice !== null && (
        <p className="m-0 text-success" role="status">
          {notice}
        </p>
      )}
      {loaded && sticks.length === 0 && unknown.length === 0 ? (
        <p className="m-0 text-fg-muted">
          No sticks yet. Plug one in and name it; reks remembers which playlists it carries.
        </p>
      ) : (
        <Card role="radiogroup" aria-label="Sticks" className="overflow-hidden">
          {/* One keyed array, so a stick moving between groups keeps its row. */}
          {[
            ...pluggedIn.map((d) => (
              <KnownStick key={`stick-${d.id}`} device={d} selected={d.id === selectedId} />
            )),
            ...unknown.map((d) => <NewStick key={`drive-${d.mountPoint}`} info={d} />),
            ...away.map((d) => (
              <KnownStick key={`stick-${d.id}`} device={d} selected={d.id === selectedId} />
            )),
          ]}
        </Card>
      )}
    </div>
  );
}

/** One line on a plugged-in volume: file system and space. */
function spaceLine(info: DeviceInfo): string {
  return `${fsLabel(info.fs)} · ${formatBytes(info.freeBytes)} free of ${formatBytes(info.totalBytes)}`;
}

function EjectButton({ info, name }: { info: DeviceInfo; name: string }) {
  const ejecting = useDevices((s) => s.ejecting);
  return (
    <Button
      size="sm"
      variant="outline"
      disabled={ejecting !== null}
      aria-label={`Eject ${name}`}
      onClick={() => void useDevices.getState().eject(info.mountPoint)}
    >
      <EjectIcon />
      {ejecting === info.mountPoint ? "Ejecting…" : "Eject"}
    </Button>
  );
}

function FsNote({ fs }: { fs: FsKind }) {
  const note = fsNote(fs);
  if (note === null) return null;
  return (
    <p className={note.tone === "danger" ? "m-0 text-xs text-danger" : "m-0 text-xs text-warning"}>
      {note.text}
    </p>
  );
}

/** A stick the library knows: a radio row, expanded with its details when picked. */
function KnownStick({ device, selected }: { device: Device; selected: boolean }) {
  const info = useProfiles((s) => {
    const mount = mountOf(s.mounted, device.id);
    return mount === null ? null : (s.mounted[mount]?.info ?? null);
  });
  return (
    <div
      data-selected={selected || undefined}
      className="flex flex-col gap-3 px-4 py-3 data-[selected]:bg-surface"
    >
      <div className="flex items-center gap-3">
        <label className="flex min-w-0 flex-1 items-center gap-3">
          <input
            type="radio"
            name="reks-stick"
            aria-label={device.name}
            checked={selected}
            onChange={() => useProfiles.getState().select(device.id)}
            className="shrink-0 accent-accent-solid"
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-semibold">{device.name}</span>
            <span className="block truncate text-xs text-fg-muted tabular-nums">
              {info !== null ? spaceLine(info) : "Not plugged in"}
            </span>
          </span>
        </label>
        {info !== null && <EjectButton info={info} name={device.name} />}
      </div>
      {selected && <StickDetails device={device} info={info} />}
    </div>
  );
}

/** The picked stick: where it is mounted, "Plays on", capacity and what a sync changes. */
function StickDetails({ device, info }: { device: Device; info: DeviceInfo | null }) {
  const status = useProfiles((s) => s.status);
  const loading = useProfiles((s) => s.statusLoading);
  const s = status !== null && status.device.id === device.id ? status : null;
  return (
    <div className="flex flex-col gap-3 pl-7 select-text">
      {info !== null && (
        <div className="flex flex-col gap-0.5">
          <p className="m-0 truncate font-mono text-xs text-fg-subtle">{info.mountPoint}</p>
          <FsNote fs={info.fs} />
        </div>
      )}
      <Field label="Plays on" hint="The oldest player this stick must work on.">
        <Select
          aria-label="Plays on"
          value={device.target}
          onChange={(e) => void useProfiles.getState().setTarget(e.target.value as TargetProfile)}
        >
          {TARGETS.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </Select>
      </Field>
      {s === null ? (
        loading && <Progress aria-label="Reading the profile" />
      ) : (
        <StickStatus status={s} mountPoint={info?.mountPoint ?? null} />
      )}
    </div>
  );
}

const STATE_TONE: Record<SyncState["state"], string> = {
  inSync: "text-fg-muted",
  new: "text-success",
  changed: "text-warning",
  willRemove: "text-danger",
};

/** Capacity, notes and per-playlist status of the picked stick's profile. */
function StickStatus({
  status: s,
  mountPoint,
}: {
  status: ProfileStatus;
  mountPoint: string | null;
}) {
  return (
    <>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="m-0 text-sm font-semibold" role="status">
            {s.summary}
          </p>
          <p className="m-0 text-xs text-fg-muted">{s.sourceText}</p>
        </div>
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

      {s.playlists.length > 0 && (
        <div className="flex flex-col gap-1">
          <h4 className="m-0 text-xs font-semibold text-fg-muted">On the stick after a sync</h4>
          <ul
            className="scroll-pane m-0 flex max-h-60 list-none flex-col gap-1 py-0 pl-0"
            aria-label="Playlist status"
          >
            {s.playlists.map((p, i) => (
              // Two stick playlists can share a path (same names); order is stable.
              // biome-ignore lint/suspicious/noArrayIndexKey: see above
              <PlaylistRow key={i} row={p} />
            ))}
          </ul>
        </div>
      )}
    </>
  );
}

/** A plugged-in drive reks doesn't know yet: its space and the identify prompt. */
function NewStick({ info }: { info: DeviceInfo }) {
  const entry = useProfiles((s) => s.mounted[info.mountPoint]);
  const name = deviceName(info);
  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1 select-text pl-7">
          <p className="m-0 truncate text-sm font-semibold">{name}</p>
          <p className="m-0 truncate text-xs text-fg-muted tabular-nums">{spaceLine(info)}</p>
          <FsNote fs={info.fs} />
        </div>
        <EjectButton info={info} name={name} />
      </div>
      {entry?.error != null ? (
        <ErrorMessage as="p" className="m-0 text-xs" message={entry.error} />
      ) : entry?.match == null ? (
        <p className="m-0 text-xs text-fg-subtle">Identifying…</p>
      ) : (
        entry.match.type !== "known" && (
          <IdentifyPrompt mountPoint={info.mountPoint} match={entry.match} />
        )
      )}
    </div>
  );
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

/** Right column: the picked stick's playlist ticks, scrolling on their own. */
function StickPlaylists() {
  const selectedId = useProfiles((s) => s.selectedId);
  const status = useProfiles((s) => s.status);
  if (selectedId === null) {
    return (
      <p className="m-0 text-sm text-fg-muted">Pick a stick to choose the playlists it carries.</p>
    );
  }
  const frees =
    status !== null && status.device.id === selectedId
      ? new Map(status.entries.map((e) => [e.playlistId, e.freesText]))
      : undefined;
  return (
    <div className="flex min-h-0 flex-col">
      <PlaylistTicks frees={frees} />
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
