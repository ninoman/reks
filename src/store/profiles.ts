/**
 * Stick profiles (phase 2 task 15, D27): the sticks this library knows, what
 * each mounted volume is to it (`profile_identify`: known, or a prompt), and
 * the status of the stick the Devices panel shows. Identity, status and all
 * wording come from Rust; this keeps the answers and refetches them when a
 * stick comes or goes.
 */
import { create } from "zustand";

import {
  type Device,
  type DeviceInfo,
  type DeviceMatch,
  type ProfileStatus,
  profileCreate,
  profileIdentify,
  profileLink,
  profileStatus,
  profilesList,
  type TargetProfile,
} from "../lib/ipc";

/** The identify answer for one mounted volume. */
export interface Mounted {
  info: DeviceInfo;
  /** Null while identifying. */
  match: DeviceMatch | null;
  error: string | null;
}

export interface ProfilesState {
  known: Device[];
  /** By mount point. */
  mounted: Record<string, Mounted>;
  /** The stick whose status the panel shows. */
  selectedId: number | null;
  status: ProfileStatus | null;
  statusLoading: boolean;
  error: string | null;

  refreshKnown(): Promise<void>;
  /** Identifies volumes that are new since the last call; forgets unplugged ones. */
  syncMounted(devices: DeviceInfo[]): Promise<void>;
  /** Answers "New stick": remembers `mountPoint` as `name`. */
  create(mountPoint: string, name: string, target?: TargetProfile): Promise<Device | null>;
  /** Answers "Is this '<name>'?" with yes for `id`. */
  link(mountPoint: string, id: number): Promise<Device | null>;
  select(id: number | null): void;
  loadStatus(): Promise<void>;
}

/** Same stick at the same mount point: no need to identify again. */
function sameVolume(a: DeviceInfo, b: DeviceInfo): boolean {
  return a.label === b.label && a.totalBytes === b.totalBytes && a.osName === b.osName;
}

/** Mount point of the plugged-in stick known as `id`, if any. */
export function mountOf(mounted: Record<string, Mounted>, id: number): string | null {
  for (const [mountPoint, m] of Object.entries(mounted)) {
    if (m.match?.type === "known" && m.match.device.id === id) return mountPoint;
  }
  return null;
}

/** Known sticks, plugged-in ones first (otherwise in the order they were added). */
export function sortedKnown(known: Device[], mounted: Record<string, Mounted>): Device[] {
  const isMounted = (d: Device) => (mountOf(mounted, d.id) === null ? 1 : 0);
  return [...known].sort((a, b) => isMounted(a) - isMounted(b));
}

function volumeSerialOf(match: DeviceMatch | null): string | null {
  return match === null || match.type === "known" ? null : match.volumeSerial;
}

let statusSeq = 0;

export const useProfiles = create<ProfilesState>()((set, get) => ({
  known: [],
  mounted: {},
  selectedId: null,
  status: null,
  statusLoading: false,
  error: null,

  async refreshKnown() {
    try {
      const known = await profilesList();
      set({ known });
      const { selectedId } = get();
      if (selectedId !== null && !known.some((d) => d.id === selectedId)) get().select(null);
    } catch (e) {
      set({ error: `Could not list your sticks: ${String(e)}` });
    }
  },

  async syncMounted(devices) {
    const before = get().mounted;
    const next: Record<string, Mounted> = {};
    const todo: DeviceInfo[] = [];
    for (const info of devices) {
      const old = before[info.mountPoint];
      if (old !== undefined && sameVolume(old.info, info)) {
        next[info.mountPoint] = { ...old, info };
      } else {
        next[info.mountPoint] = { info, match: null, error: null };
        todo.push(info);
      }
    }
    const selected = get().selectedId;
    const selectedMount = selected === null ? null : mountOf(before, selected);
    set({ mounted: next });

    await Promise.all(
      todo.map(async (info) => {
        let entry: Mounted;
        try {
          entry = { info, match: await profileIdentify(info), error: null };
        } catch (e) {
          entry = { info, match: null, error: `Could not identify the stick: ${String(e)}` };
        }
        const cur = get().mounted[info.mountPoint];
        // Unplugged or replaced meanwhile: drop the answer.
        if (cur === undefined || cur.match !== null || !sameVolume(cur.info, info)) return;
        set({ mounted: { ...get().mounted, [info.mountPoint]: entry } });
      }),
    );

    const s = get();
    if (s.selectedId === null) {
      // Show a plugged-in known stick first, else the first known one.
      const first = sortedKnown(s.known, s.mounted)[0];
      if (first !== undefined) s.select(first.id);
    } else if (mountOf(s.mounted, s.selectedId) !== selectedMount) {
      // Plugged in or out: the status source changes (stick vs last sync).
      void s.loadStatus();
    }
  },

  async create(mountPoint, name, target = "Generic") {
    const m = get().mounted[mountPoint];
    if (m === undefined) return null;
    try {
      const device = await profileCreate(m.info, volumeSerialOf(m.match), name.trim(), target);
      settle(mountPoint, device);
      return device;
    } catch (e) {
      set({ error: `Could not save the stick: ${String(e)}` });
      return null;
    }
  },

  async link(mountPoint, id) {
    const m = get().mounted[mountPoint];
    if (m === undefined) return null;
    try {
      const device = await profileLink(id, m.info, volumeSerialOf(m.match));
      settle(mountPoint, device);
      return device;
    } catch (e) {
      set({ error: `Could not link the stick: ${String(e)}` });
      return null;
    }
  },

  select(id) {
    set({ selectedId: id, status: null, error: null });
    if (id !== null) void get().loadStatus();
  },

  async loadStatus() {
    const { selectedId: id, mounted } = get();
    if (id === null) return;
    const mine = ++statusSeq;
    set({ statusLoading: true });
    try {
      const status = await profileStatus(id, mountOf(mounted, id));
      if (mine === statusSeq) set({ status, statusLoading: false, error: null });
    } catch (e) {
      if (mine === statusSeq) {
        set({ statusLoading: false, error: `Could not read the profile: ${String(e)}` });
      }
    }
  },
}));

/** After create/link: the volume is now known as `device`. */
function settle(mountPoint: string, device: Device) {
  const s = useProfiles.getState();
  const m = s.mounted[mountPoint];
  if (m !== undefined) {
    useProfiles.setState({
      mounted: { ...s.mounted, [mountPoint]: { ...m, match: { type: "known", device } } },
      error: null,
    });
  }
  void s.refreshKnown().then(() => useProfiles.getState().select(device.id));
}
