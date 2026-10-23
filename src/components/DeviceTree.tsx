/**
 * The Devices tree in the sidebar, above the Histories: the plugged-in
 * drives, each with the playlists and the play history its rekordbox library
 * holds (read each time the drive is opened). Click a playlist or history
 * session (or Up/Down with the tree focused) to show it in the main area,
 * where its tracks play off the stick. The gear opens the USB devices
 * dialog; a drive's eject button ejects it.
 */
import type { KeyboardEvent, ReactNode } from "react";

import type { DeviceInfo, StickLibrary, StickPlaylist } from "../lib/ipc";
import {
  childrenOf,
  type LibraryLoad,
  nodeKey,
  type StickListRef,
  useDeviceBrowser,
  useDevicePolling,
} from "../store/deviceBrowser";
import { deviceName, useDevices } from "../store/devices";
import { Disclosure } from "./Disclosure";
import { ReportErrorButton } from "./ErrorMessage";
import { EjectIcon, FolderIcon, GearIcon, PlaylistIcon, UsbIcon } from "./icons";
import { ICON, INDENT, NAME, PAD, ROW } from "./treeStyles";
import { Button } from "./ui/Button";
import { focusRingInset } from "./ui/control";
import { IconButton } from "./ui/IconButton";
import { ScrollArea } from "./ui/ScrollArea";

const indent = (level: number) => ({ paddingLeft: PAD + (level - 1) * INDENT });

export function DeviceTree() {
  const treeOpen = useDeviceBrowser((s) => s.treeOpen);
  const expanded = useDeviceBrowser((s) => s.expanded);
  const libraries = useDeviceBrowser((s) => s.libraries);
  const open = useDeviceBrowser((s) => s.open);
  const devices = useDevices((s) => s.devices);
  const loaded = useDevices((s) => s.loaded);
  const b = useDeviceBrowser.getState;
  useDevicePolling(treeOpen);

  /** The lists the tree shows now, in order. */
  function visibleLists(): StickListRef[] {
    if (!treeOpen) return [];
    return devices.flatMap((d) => {
      const load = libraries[d.mountPoint];
      if (!expanded.has(nodeKey.device(d.mountPoint)) || load?.state !== "ready") return [];
      const lib = load.library;
      if (lib === null) return [];
      const mountPoint = d.mountPoint;
      const playlists = (parentId: number | null): StickListRef[] =>
        childrenOf(lib.playlists, parentId).flatMap((p) => {
          if (!p.isFolder) return [{ mountPoint, kind: "playlist" as const, id: p.id }];
          return expanded.has(nodeKey.folder(mountPoint, p.id)) ? playlists(p.id) : [];
        });
      return [
        ...(expanded.has(nodeKey.playlists(mountPoint)) ? playlists(null) : []),
        ...(expanded.has(nodeKey.histories(mountPoint))
          ? lib.histories.map((h) => ({ mountPoint, kind: "history" as const, id: h.id }))
          : []),
      ];
    });
  }

  /** Up/Down open the previous/next list among the visible ones. */
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const lists = visibleLists();
    const i = lists.findIndex((l) => sameList(l, open));
    const next = lists[i === -1 ? 0 : i + (e.key === "ArrowDown" ? 1 : -1)];
    if (next !== undefined) b().openList(next);
  }

  return (
    <section
      className="flex max-h-1/3 min-h-0 shrink-0 flex-col border-t border-border"
      aria-label="Devices"
    >
      <ScrollArea className="min-h-0">
        <div
          className={`flex flex-col py-1 ${focusRingInset}`}
          role="tree"
          aria-label="Devices"
          tabIndex={0}
          onKeyDown={onKeyDown}
        >
          <div
            role="treeitem"
            aria-level={1}
            aria-selected={false}
            aria-expanded={treeOpen}
            className={ROW}
            data-root=""
            onClick={() => b().toggleTree()}
          >
            <Disclosure open={treeOpen} />
            <span className={NAME}>Devices</span>
            <IconButton
              size="xs"
              shape="round"
              title="USB devices…"
              aria-label="USB devices"
              onClick={(e) => {
                e.stopPropagation();
                useDevices.getState().setDialogOpen(true);
              }}
            >
              <GearIcon size={14} />
            </IconButton>
          </div>
          {treeOpen && loaded && devices.length === 0 && (
            <Note level={2}>No USB drive plugged in.</Note>
          )}
          {treeOpen &&
            devices.map((d) => (
              <DeviceNode
                key={d.mountPoint}
                device={d}
                load={libraries[d.mountPoint]}
                expanded={expanded}
                open={open}
              />
            ))}
        </div>
      </ScrollArea>
    </section>
  );
}

function sameList(a: StickListRef, b: StickListRef | null): boolean {
  return b !== null && a.mountPoint === b.mountPoint && a.kind === b.kind && a.id === b.id;
}

function DeviceNode({
  device,
  load,
  expanded,
  open,
}: {
  device: DeviceInfo;
  load: LibraryLoad | undefined;
  expanded: ReadonlySet<string>;
  open: StickListRef | null;
}) {
  const ejecting = useDevices((s) => s.ejecting);
  const b = useDeviceBrowser.getState;
  const mountPoint = device.mountPoint;
  const isOpen = expanded.has(nodeKey.device(mountPoint));
  const name = deviceName(device);
  return (
    <>
      <div
        role="treeitem"
        aria-level={2}
        aria-selected={false}
        aria-expanded={isOpen}
        className={`${ROW} group`}
        style={indent(2)}
        title={mountPoint}
        onClick={() => b().toggleDevice(mountPoint)}
      >
        <Disclosure open={isOpen} />
        <span className={ICON}>
          <UsbIcon />
        </span>
        <span className={NAME}>{name}</span>
        <IconButton
          size="xs"
          shape="round"
          className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          title={`Eject ${name}`}
          aria-label={`Eject ${name}`}
          disabled={ejecting !== null}
          onClick={(e) => {
            e.stopPropagation();
            void b().eject(mountPoint);
          }}
        >
          <EjectIcon />
        </IconButton>
      </div>
      {isOpen && (
        <DeviceContents mountPoint={mountPoint} load={load} expanded={expanded} open={open} />
      )}
    </>
  );
}

function DeviceContents({
  mountPoint,
  load,
  expanded,
  open,
}: {
  mountPoint: string;
  load: LibraryLoad | undefined;
  expanded: ReadonlySet<string>;
  open: StickListRef | null;
}) {
  if (load === undefined || load.state === "loading") return <Note level={3}>Reading…</Note>;
  if (load.state === "error") {
    return (
      <div className="flex flex-col items-start gap-1 py-1 pr-2" style={indent(3)} role="alert">
        <span className="text-xs text-danger select-text">{load.message}</span>
        <span className="flex gap-1.5">
          <Button
            size="xs"
            variant="link"
            onClick={() => void useDeviceBrowser.getState().reload(mountPoint)}
          >
            Try again
          </Button>
          <ReportErrorButton message={load.message} />
        </span>
      </div>
    );
  }
  if (load.library === null) return <Note level={3}>No rekordbox library on this drive.</Note>;
  return (
    <LibraryNodes mountPoint={mountPoint} library={load.library} expanded={expanded} open={open} />
  );
}

function LibraryNodes({
  mountPoint,
  library,
  expanded,
  open,
}: {
  mountPoint: string;
  library: StickLibrary;
  expanded: ReadonlySet<string>;
  open: StickListRef | null;
}) {
  const b = useDeviceBrowser.getState;
  const listRow = (ref: StickListRef, name: string, level: number) => {
    const selected = sameList(ref, open);
    return (
      <div
        key={`${ref.kind}${ref.id}`}
        role="treeitem"
        aria-level={level}
        aria-selected={selected}
        className={ROW}
        data-selected={selected || undefined}
        style={indent(level)}
        onClick={() => b().openList(ref)}
      >
        <span className="inline-flex w-3.5 flex-none" />
        {ref.kind === "playlist" && <PlaylistIcon className={ICON} />}
        <span className={NAME}>{name}</span>
      </div>
    );
  };
  const group = (key: string, label: string, level: number, count?: number, folder = false) => (
    <div
      key={key}
      role="treeitem"
      aria-level={level}
      aria-selected={false}
      aria-expanded={expanded.has(key)}
      className={ROW}
      style={indent(level)}
      onClick={() => b().toggleNode(key)}
    >
      <Disclosure open={expanded.has(key)} />
      {folder && <FolderIcon className={ICON} />}
      <span className={NAME}>{label}</span>
      {count !== undefined && <span className="text-xs text-fg-subtle tabular-nums">{count}</span>}
    </div>
  );
  const playlists = (parentId: number | null, level: number): ReactNode[] =>
    childrenOf(library.playlists, parentId).flatMap((p: StickPlaylist) => {
      if (!p.isFolder) {
        return [listRow({ mountPoint, kind: "playlist", id: p.id }, p.name, level)];
      }
      const key = nodeKey.folder(mountPoint, p.id);
      return [
        group(key, p.name, level, undefined, true),
        ...(expanded.has(key) ? playlists(p.id, level + 1) : []),
      ];
    });

  const playlistsKey = nodeKey.playlists(mountPoint);
  const historiesKey = nodeKey.histories(mountPoint);
  return (
    <>
      {group(playlistsKey, "Playlists", 3)}
      {expanded.has(playlistsKey) &&
        (library.playlists.length === 0 ? (
          <Note level={4}>No playlists.</Note>
        ) : (
          playlists(null, 4)
        ))}
      {group(historiesKey, "Histories", 3, library.histories.length)}
      {expanded.has(historiesKey) &&
        (library.histories.length === 0 ? (
          <Note level={4}>Not played on a player yet.</Note>
        ) : (
          library.histories.map((h) =>
            listRow({ mountPoint, kind: "history", id: h.id }, h.name, 4),
          )
        ))}
    </>
  );
}

function Note({ level, children }: { level: number; children: ReactNode }) {
  return (
    <p className="m-0 py-1 pr-2 text-xs text-fg-subtle" style={indent(level)}>
      {children}
    </p>
  );
}
