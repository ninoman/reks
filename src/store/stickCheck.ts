/**
 * Check a stick before a gig (Zustand): each drive's last report, read by
 * `device_check`. Which report the main area shows is `useDeviceBrowser`'s
 * `checking`, so it closes with the stick lists. Read-only on the stick.
 */
import { create } from "zustand";

import { type DeviceInfo, deviceCheck, type StickReport } from "../lib/ipc";

export type CheckLoad =
  | { state: "loading" }
  | { state: "ready"; report: StickReport }
  | { state: "error"; message: string };

export interface StickCheckState {
  /** By mount point. */
  loads: Readonly<Record<string, CheckLoad>>;
  /** Checks the drive (again); the newest run wins. */
  run(device: DeviceInfo): Promise<void>;
  /** Forgets reports of drives that went away. */
  forget(mountPoints: readonly string[]): void;
}

let lastRun = 0;
const runs: Record<string, number> = {};

export const useStickCheck = create<StickCheckState>()((set) => ({
  loads: {},

  async run(device) {
    const mountPoint = device.mountPoint;
    const id = ++lastRun;
    runs[mountPoint] = id;
    set((s) => ({ loads: { ...s.loads, [mountPoint]: { state: "loading" } } }));
    let load: CheckLoad;
    try {
      load = { state: "ready", report: await deviceCheck(device) };
    } catch (e) {
      load = { state: "error", message: String(e) };
    }
    if (runs[mountPoint] !== id) return;
    set((s) => ({ loads: { ...s.loads, [mountPoint]: load } }));
  },

  forget(mountPoints) {
    if (mountPoints.length === 0) return;
    set((s) => {
      const loads = { ...s.loads };
      for (const m of mountPoints) {
        delete loads[m];
        delete runs[m];
      }
      return { loads };
    });
  },
}));
