/**
 * USB devices (phase 2 task 1): the removable volumes the OS reports, kept
 * fresh while the devices dialog is open, and ejecting one. The export
 * wizard (task 10) picks its target from the same list.
 */
import { create } from "zustand";

import { type DeviceInfo, deviceEject, devicesList } from "../lib/ipc";

export interface DevicesState {
  dialogOpen: boolean;
  devices: DeviceInfo[];
  /** False until the first list arrives. */
  loaded: boolean;
  error: string | null;
  /** Mount point being ejected. */
  ejecting: string | null;
  /** Result of the last eject, e.g. "REKS ejected. You can remove it." */
  notice: string | null;
  setDialogOpen(open: boolean): void;
  refresh(): Promise<void>;
  eject(mountPoint: string): Promise<void>;
}

/** Display name: the label, else the last part of the mount point. */
export function deviceName(d: DeviceInfo): string {
  return d.label !== ""
    ? d.label
    : (d.mountPoint.split(/[\\/]/).filter(Boolean).pop() ?? d.mountPoint);
}

let seq = 0;

export const useDevices = create<DevicesState>()((set, get) => ({
  dialogOpen: false,
  devices: [],
  loaded: false,
  error: null,
  ejecting: null,
  notice: null,
  setDialogOpen(open) {
    set(open ? { dialogOpen: true, notice: null, error: null } : { dialogOpen: false });
  },
  async refresh() {
    const mine = ++seq;
    try {
      const devices = await devicesList();
      if (mine === seq) set({ devices, loaded: true, error: null });
    } catch (e) {
      if (mine === seq) set({ loaded: true, error: `Could not list devices: ${String(e)}` });
    }
  },
  async eject(mountPoint) {
    if (get().ejecting !== null) return;
    const device = get().devices.find((d) => d.mountPoint === mountPoint);
    const name = device ? deviceName(device) : mountPoint;
    set({ ejecting: mountPoint, notice: null, error: null });
    try {
      await deviceEject(mountPoint);
      set({ notice: `${name} ejected. You can remove it.` });
    } catch (e) {
      set({ error: `Could not eject ${name}: ${String(e)}` });
    } finally {
      set({ ejecting: null });
      await get().refresh();
    }
  },
}));
