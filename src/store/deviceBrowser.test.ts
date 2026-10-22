import { afterEach, describe, expect, it, vi } from "vitest";

import type { DeviceInfo, StickLibrary, StickTrack } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  deviceLibrary: vi.fn(),
  historySession: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { childrenOf, nodeKey, openRows, stickRef, useDeviceBrowser } = await import(
  "./deviceBrowser"
);
const { useHistory } = await import("./history");

const MOUNT = "/Volumes/REKS";
const initial = useDeviceBrowser.getState();

function track(id: number): StickTrack {
  return {
    id,
    title: `T${id}`,
    artist: "A",
    album: null,
    genre: null,
    bpmX100: 12800,
    key: "Am",
    durationMs: 300_000,
    stickPath: `/Contents/A/T${id}.mp3`,
  };
}

const library: StickLibrary = {
  tracks: [track(1), track(2)],
  playlists: [
    { id: 10, parentId: null, name: "Folder", isFolder: true, trackIds: [] },
    { id: 11, parentId: 10, name: "Inside", isFolder: false, trackIds: [2, 1] },
    { id: 12, parentId: null, name: "Top", isFolder: false, trackIds: [1] },
  ],
  histories: [{ id: 1, name: "HISTORY 001", trackIds: [1, 99] }],
};

const device = (mountPoint: string): DeviceInfo => ({
  mountPoint,
  fs: { kind: "fat32" },
  fsName: "msdos",
  removable: true,
  totalBytes: 1,
  freeBytes: 1,
  label: "",
  osName: "",
});

afterEach(() => {
  vi.clearAllMocks();
  useDeviceBrowser.setState(initial, true);
  useHistory.setState({ openId: null, detail: null });
});

describe("device browser", () => {
  it("lists children in order and resolves list rows, missing tracks too", () => {
    expect(childrenOf(library.playlists, null).map((p) => p.name)).toEqual(["Folder", "Top"]);
    expect(childrenOf(library.playlists, 10).map((p) => p.name)).toEqual(["Inside"]);

    const inside = openRows(library, { mountPoint: MOUNT, kind: "playlist", id: 11 });
    expect(inside?.rows.map((r) => [r.n, r.track?.id])).toEqual([
      [1, 2],
      [2, 1],
    ]);
    const history = openRows(library, { mountPoint: MOUNT, kind: "history", id: 1 });
    expect(history?.rows[1]).toEqual({ n: 2, trackId: 99, track: null });
    // A folder is not a list.
    expect(openRows(library, { mountPoint: MOUNT, kind: "playlist", id: 10 })).toBeNull();
  });

  it("makes a stick load request", () => {
    expect(stickRef(MOUNT, track(2))).toEqual({
      kind: "stick",
      mountPoint: MOUNT,
      stickId: 2,
      title: "T2",
      artist: "A",
      key: "Am",
    });
  });

  it("reads a drive's library the first time it opens, then opens its playlists", async () => {
    vi.mocked(ipc.deviceLibrary).mockResolvedValue(library);
    useDeviceBrowser.getState().toggleDevice(MOUNT);
    expect(useDeviceBrowser.getState().libraries[MOUNT]).toEqual({ state: "loading" });
    await vi.waitFor(() =>
      expect(useDeviceBrowser.getState().libraries[MOUNT]).toEqual({ state: "ready", library }),
    );
    expect(useDeviceBrowser.getState().expanded.has(nodeKey.playlists(MOUNT))).toBe(true);

    // Opening it again reads it again, showing the last read meanwhile.
    const changed = { ...library, histories: [] };
    vi.mocked(ipc.deviceLibrary).mockResolvedValue(changed);
    useDeviceBrowser.getState().toggleDevice(MOUNT);
    useDeviceBrowser.getState().toggleDevice(MOUNT);
    expect(useDeviceBrowser.getState().libraries[MOUNT]).toEqual({ state: "ready", library });
    await vi.waitFor(() =>
      expect(useDeviceBrowser.getState().libraries[MOUNT]).toEqual({
        state: "ready",
        library: changed,
      }),
    );
    expect(ipc.deviceLibrary).toHaveBeenCalledTimes(2);
  });

  it("keeps a read error for Try again", async () => {
    vi.mocked(ipc.deviceLibrary).mockRejectedValue("export.pdb: bad page");
    useDeviceBrowser.getState().toggleDevice(MOUNT);
    await vi.waitFor(() =>
      expect(useDeviceBrowser.getState().libraries[MOUNT]).toEqual({
        state: "error",
        message: "export.pdb: bad page",
      }),
    );
  });

  it("opening a stick list closes the history session, and vice versa", async () => {
    vi.mocked(ipc.historySession).mockResolvedValue(
      {} as Awaited<ReturnType<typeof ipc.historySession>>,
    );
    useHistory.setState({ openId: 4 });
    useDeviceBrowser.getState().openList({ mountPoint: MOUNT, kind: "playlist", id: 12 });
    expect(useHistory.getState().openId).toBeNull();

    await useHistory.getState().open(4);
    expect(useDeviceBrowser.getState().open).toBeNull();
  });

  it("forgets a drive that went away and closes its open list", () => {
    useDeviceBrowser.setState({
      libraries: { [MOUNT]: { state: "ready", library }, "/Volumes/B": { state: "loading" } },
      expanded: new Set([
        nodeKey.device(MOUNT),
        nodeKey.folder(MOUNT, 10),
        nodeKey.device("/Volumes/B"),
      ]),
      open: { mountPoint: MOUNT, kind: "playlist", id: 12 },
    });
    useDeviceBrowser.getState().syncDevices([device("/Volumes/B")]);
    const s = useDeviceBrowser.getState();
    expect(Object.keys(s.libraries)).toEqual(["/Volumes/B"]);
    expect([...s.expanded]).toEqual([nodeKey.device("/Volumes/B")]);
    expect(s.open).toBeNull();
  });
});
