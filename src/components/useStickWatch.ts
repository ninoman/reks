/**
 * Keeps the plugged-in volumes and their identities fresh while the app
 * runs, so the header can offer "Sync <name>" (D42). The Sticks view polls
 * faster on its own while open.
 */
import { useEffect } from "react";

import type { Device, DeviceInfo } from "../lib/ipc";
import { useDevices } from "../store/devices";
import { useProfiles } from "../store/profiles";

const WATCH_MS = 4000;

export interface PluggedSticks {
  devices: DeviceInfo[];
  /** The one plugged-in volume, when there is exactly one. */
  only: DeviceInfo | null;
  /** That volume's profile, when it is a known stick. */
  onlyKnown: Device | null;
}

export function useStickWatch(): PluggedSticks {
  const devices = useDevices((s) => s.devices);
  const loaded = useDevices((s) => s.loaded);
  const only = devices.length === 1 ? (devices[0] ?? null) : null;
  const onlyKnown = useProfiles((s) => {
    const match = only === null ? undefined : s.mounted[only.mountPoint]?.match;
    return match?.type === "known" ? match.device : null;
  });

  useEffect(() => {
    void useProfiles.getState().refreshKnown();
    void useDevices.getState().refresh();
    const timer = window.setInterval(() => void useDevices.getState().refresh(), WATCH_MS);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (loaded) void useProfiles.getState().syncMounted(devices);
  }, [devices, loaded]);

  return { devices, only, onlyKnown };
}
