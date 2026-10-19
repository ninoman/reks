/**
 * USB devices (phase 2 task 1): the removable volumes the OS reports, with
 * their file system and free space, and Eject. The list refreshes every two
 * seconds while open, so a stick shows up when it is plugged in.
 * `DeviceList` is also the export wizard's device step (task 10): pass
 * `selected` and `onSelect` to pick one. Whether a file system will do for a
 * given player is the export planner's call; the notes here only flag it.
 */
import { useEffect } from "react";

import { formatBytes } from "../lib/format";
import type { DeviceInfo, FsKind } from "../lib/ipc";
import { deviceName, useDevices } from "../store/devices";
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
      <DialogContent size="md">
        <DialogHeader>
          <DialogTitle>USB devices</DialogTitle>
          <DialogDescription>Removable drives connected to this computer.</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <DeviceList />
        </DialogBody>
        <DialogFooter>
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
    void refresh();
    const timer = window.setInterval(() => void refresh(), REFRESH_MS);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <div className="flex flex-col gap-3">
      {error !== null && (
        <p className="m-0 text-danger select-text" role="alert">
          {error}
        </p>
      )}
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
  onSelect?: (device: DeviceInfo) => void;
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
      className="flex items-center gap-3 px-4 py-3 data-[selected]:bg-selected data-[selected]:text-selected-fg"
    >
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
  );
}
