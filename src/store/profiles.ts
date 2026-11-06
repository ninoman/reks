/**
 * Stick profiles (phase 2 task 15, D27): the sticks this library knows, what
 * each mounted volume is to it (`profile_identify`: known, or a prompt), and
 * the status of the stick the Sticks view shows, with its playlist ticks
 * and "Plays on" (D42: the profile is edited here, not in the export).
 * Ticks and the target are saved as they change. Identity, status and all
 * wording come from Rust; this keeps the answers and refetches them when a
 * stick comes or goes.
 */
import { create } from "zustand";

import {
  type Device,
  type DeviceInfo,
  type DeviceMatch,
  type ExportSelection,
  type PlaylistNode,
  type ProfileStatus,
  profileCreate,
  profileIdentify,
  profileLink,
  profileSelection,
  profileSetSelection,
  profileSetTarget,
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
  /** The selected stick's ticked playlists and folders; null while loading. */
  ticks: ReadonlySet<number> | null;
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
  /** Ticks or unticks a playlist or folder of the selected stick and saves the profile. */
  toggleTick(id: number, nodes: PlaylistNode[]): Promise<void>;
  /** Saves the selected stick's "Plays on". */
  setTarget(target: TargetProfile): Promise<void>;
}

/** Ids that a ticked folder above them already covers. */
export function coveredIds(nodes: PlaylistNode[], checked: ReadonlySet<number>): Set<number> {
  const covered = new Set<number>();
  // Depth first: a parent comes before its children.
  for (const n of nodes) {
    if (n.parentId !== null && (checked.has(n.parentId) || covered.has(n.parentId))) {
      covered.add(n.id);
    }
  }
  return covered;
}

/** The ticked nodes no ticked folder covers, in tree order: the `Selection`. */
export function selectionOf(nodes: PlaylistNode[], checked: ReadonlySet<number>): ExportSelection {
  const covered = coveredIds(nodes, checked);
  return {
    playlists: nodes.filter((n) => checked.has(n.id) && !covered.has(n.id)).map((n) => n.id),
  };
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
let ticksSeq = 0;

export const useProfiles = create<ProfilesState>()((set, get) => ({
  known: [],
  mounted: {},
  selectedId: null,
  status: null,
  statusLoading: false,
  ticks: null,
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
    const mine = ++ticksSeq;
    set({ selectedId: id, status: null, ticks: null, error: null });
    if (id === null) return;
    void get().loadStatus();
    void (async () => {
      try {
        const sel = await profileSelection(id);
        if (mine === ticksSeq) set({ ticks: new Set(sel.playlists) });
      } catch (e) {
        if (mine === ticksSeq) set({ error: `Could not read the stick's playlists: ${String(e)}` });
      }
    })();
  },

  async toggleTick(nodeId, nodes) {
    const { selectedId: id, ticks } = get();
    if (id === null || ticks === null) return;
    const next = new Set(ticks);
    if (!next.delete(nodeId)) next.add(nodeId);
    set({ ticks: next });
    try {
      await profileSetSelection(id, selectionOf(nodes, next).playlists);
      await get().loadStatus();
    } catch (e) {
      set({ ticks, error: `Could not save the stick's playlists: ${String(e)}` });
    }
  },

  async setTarget(target) {
    const id = get().selectedId;
    if (id === null) return;
    try {
      const device = await profileSetTarget(id, target);
      set({ known: get().known.map((d) => (d.id === id ? device : d)) });
      // A known stick's identity carries its device; keep the mounted copy in step.
      const mount = mountOf(get().mounted, id);
      const m = mount === null ? undefined : get().mounted[mount];
      if (mount !== null && m !== undefined) {
        set({ mounted: { ...get().mounted, [mount]: { ...m, match: { type: "known", device } } } });
      }
      await get().loadStatus();
    } catch (e) {
      set({ error: `Could not save "Plays on": ${String(e)}` });
    }
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
