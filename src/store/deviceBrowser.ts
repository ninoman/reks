/**
 * The sidebar's Devices section (Zustand): the plugged-in drives (`useDevices`),
 * each one's stick library read when it is first expanded, which tree nodes
 * are open, and the stick playlist, history or check report shown in the
 * main area instead of the track table or a library history session.
 * Read-only: a stick track plays on the player deck straight off the stick
 * (`stickRef`).
 */
import { useEffect } from "react";
import { create } from "zustand";

import {
  type DeviceInfo,
  deviceLibrary,
  type StickLibrary,
  type StickPlaylist,
  type StickTrack,
} from "../lib/ipc";
import { useDevices } from "./devices";
import { useHistory } from "./history";
import type { TrackRef } from "./mix";
import { reportAction } from "./report";
import { useStickCheck } from "./stickCheck";
import { useToasts } from "./toasts";

/** How often the list of drives is refreshed while the section is open. */
const POLL_MS = 2000;

/** A stick playlist or history session, shown in the main area. */
export interface StickListRef {
  mountPoint: string;
  kind: "playlist" | "history";
  id: number;
}

export type LibraryLoad =
  | { state: "loading" }
  /** `library` null: the drive has no rekordbox library. */
  | { state: "ready"; library: StickLibrary | null }
  | { state: "error"; message: string };

/** One row of an open stick list; `track` null when the stick no longer has it. */
export interface StickRow {
  /** 1-based position in the list. */
  n: number;
  trackId: number;
  track: StickTrack | null;
}

export interface DeviceBrowserState {
  treeOpen: boolean;
  /** Open nodes ([`nodeKey`]). */
  expanded: ReadonlySet<string>;
  /** By mount point. */
  libraries: Readonly<Record<string, LibraryLoad>>;
  open: StickListRef | null;
  /** Mount point of the drive whose check report the main area shows. */
  checking: string | null;
  toggleTree(): void;
  /**
   * Opens or closes a drive. Opening reads its library: shown as loading the
   * first time, and quietly again later (an export may have changed it).
   */
  toggleDevice(mountPoint: string): void;
  toggleNode(key: string): void;
  /** Reads the drive's library again; `quiet` keeps showing the last read meanwhile. */
  reload(mountPoint: string, quiet?: boolean): Promise<void>;
  openList(ref: StickListRef): void;
  /** Shows the drive's check report and checks it (again). */
  openCheck(device: DeviceInfo): void;
  /** Closes the stick list or check report. */
  close(): void;
  /** Ejects the drive; the outcome shows as a toast. */
  eject(mountPoint: string): Promise<void>;
  /** The drives the OS reports now: those that went away are forgotten. */
  syncDevices(devices: readonly DeviceInfo[]): void;
}

/** Tree node keys: a drive, its Playlists or Histories group, or a playlist folder. */
export const nodeKey = {
  device: (mountPoint: string) => `d\u0000${mountPoint}`,
  playlists: (mountPoint: string) => `p\u0000${mountPoint}`,
  histories: (mountPoint: string) => `h\u0000${mountPoint}`,
  folder: (mountPoint: string, id: number) => `f\u0000${mountPoint}\u0000${id}`,
};

/** The playlists under `parentId` (null: the root), in player order. */
export function childrenOf(
  playlists: readonly StickPlaylist[],
  parentId: number | null,
): StickPlaylist[] {
  return playlists.filter((p) => p.parentId === parentId);
}

/** The list `ref` names in `library`, its rows resolved; null when the stick has no such list. */
export function openRows(
  library: StickLibrary,
  ref: StickListRef,
): { name: string; rows: StickRow[] } | null {
  const list =
    ref.kind === "playlist"
      ? library.playlists.find((p) => p.id === ref.id && !p.isFolder)
      : library.histories.find((h) => h.id === ref.id);
  if (list === undefined) return null;
  const byId = new Map(library.tracks.map((t) => [t.id, t]));
  return {
    name: list.name,
    rows: list.trackIds.map((trackId, i) => ({
      n: i + 1,
      trackId,
      track: byId.get(trackId) ?? null,
    })),
  };
}

/** `track` on the stick at `mountPoint` as a load request. */
export function stickRef(mountPoint: string, track: StickTrack): TrackRef {
  return {
    kind: "stick",
    mountPoint,
    stickId: track.id,
    title: track.title,
    artist: track.artist,
    key: track.key,
  };
}

function toggled(set: ReadonlySet<string>, key: string): Set<string> {
  const next = new Set(set);
  if (!next.delete(key)) next.add(key);
  return next;
}

export const useDeviceBrowser = create<DeviceBrowserState>()((set, get) => ({
  treeOpen: true,
  expanded: new Set(),
  libraries: {},
  open: null,
  checking: null,

  toggleTree() {
    set((s) => ({ treeOpen: !s.treeOpen }));
  },

  toggleDevice(mountPoint) {
    const key = nodeKey.device(mountPoint);
    const opening = !get().expanded.has(key);
    set((s) => ({ expanded: toggled(s.expanded, key) }));
    if (opening) void get().reload(mountPoint, get().libraries[mountPoint]?.state === "ready");
  },

  toggleNode(key) {
    set((s) => ({ expanded: toggled(s.expanded, key) }));
  },

  async reload(mountPoint, quiet = false) {
    if (!quiet) {
      set((s) => ({ libraries: { ...s.libraries, [mountPoint]: { state: "loading" } } }));
    }
    let load: LibraryLoad;
    try {
      load = { state: "ready", library: await deviceLibrary(mountPoint) };
    } catch (e) {
      load = { state: "error", message: String(e) };
    }
    // Unplugged meanwhile: nothing to show it under.
    if (get().libraries[mountPoint] === undefined) return;
    set((s) => {
      const libraries = { ...s.libraries, [mountPoint]: load };
      if (!quiet && load.state === "ready" && load.library !== null) {
        // Open the Playlists group the first time a library shows up.
        return {
          libraries,
          expanded: new Set([...s.expanded, nodeKey.playlists(mountPoint)]),
        };
      }
      return { libraries };
    });
  },

  openList(ref) {
    useHistory.getState().close();
    set({ open: ref, checking: null });
  },

  openCheck(device) {
    useHistory.getState().close();
    set({ open: null, checking: device.mountPoint });
    void useStickCheck.getState().run(device);
  },

  close() {
    if (get().open !== null || get().checking !== null) set({ open: null, checking: null });
  },

  async eject(mountPoint) {
    const devices = useDevices.getState();
    await devices.eject(mountPoint);
    const { notice, error } = useDevices.getState();
    if (error !== null) useToasts.getState().show(error, reportAction(error));
    else if (notice !== null) useToasts.getState().show(notice);
  },

  syncDevices(devices) {
    const present = new Set(devices.map((d) => d.mountPoint));
    const s = get();
    const gone = Object.keys(s.libraries).filter((m) => !present.has(m));
    const openGone = s.open !== null && !present.has(s.open.mountPoint);
    const checkGone = s.checking !== null && !present.has(s.checking);
    useStickCheck.getState().forget(gone);
    if (gone.length === 0 && !openGone && !checkGone) return;
    const libraries = { ...s.libraries };
    for (const m of gone) delete libraries[m];
    // A drive plugged in again starts closed: its library may have changed.
    const expanded = new Set(
      [...s.expanded].filter((k) => !gone.some((m) => k.split("\u0000")[1] === m)),
    );
    set({
      libraries,
      expanded,
      open: openGone ? null : s.open,
      checking: checkGone ? null : s.checking,
    });
  },
}));

/** Keeps the drive list fresh while `active`, and the browser in step with it. */
export function useDevicePolling(active: boolean) {
  const devices = useDevices((s) => s.devices);
  const loaded = useDevices((s) => s.loaded);

  useEffect(() => {
    if (!active) return;
    const { refresh } = useDevices.getState();
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    return () => window.clearInterval(timer);
  }, [active]);

  useEffect(() => {
    if (loaded) useDeviceBrowser.getState().syncDevices(devices);
  }, [devices, loaded]);
}
