/**
 * A stick playlist or history session in the main area (sidebar Devices
 * section), in the stick's order. Double-click a track, or its play button
 * (shown on hover and focus), to play it on the player deck straight off the
 * stick, with the stick's grid and cues. Entries whose track the stick no longer has are
 * greyed. Read-only: nothing here changes the stick or the library.
 */
import { formatBpm, formatDuration } from "../lib/format";
import type { StickTrack } from "../lib/ipc";
import { openRows, type StickRow, stickRef, useDeviceBrowser } from "../store/deviceBrowser";
import { deviceName, useDevices } from "../store/devices";
import { activateTrack, playTrack, usePlayerHas } from "../store/player";
import { PlayIcon } from "./icons";
import { KeyLabel } from "./KeyLabel";
import { IconButton } from "./ui/IconButton";
import { ScrollArea } from "./ui/ScrollArea";

const TH = "sticky top-0 bg-bg px-2 py-1 text-left text-xs font-normal text-fg-muted";
const TD = "h-(--row-h) px-2 whitespace-nowrap overflow-hidden text-ellipsis";

export function StickListView() {
  const open = useDeviceBrowser((s) => s.open);
  const load = useDeviceBrowser((s) => (open === null ? undefined : s.libraries[open.mountPoint]));
  const device = useDevices((s) => s.devices.find((d) => d.mountPoint === open?.mountPoint));

  if (open === null) return null;
  const status = (text: string, alert = false) => (
    <section
      className="flex flex-1 items-center justify-center text-fg-muted"
      role={alert ? "alert" : "status"}
    >
      {text}
    </section>
  );
  if (load === undefined || load.state === "loading") return status("Reading the stick…");
  if (load.state === "error") return status(load.message, true);
  const list = load.library === null ? null : openRows(load.library, open);
  if (list === null) return status("This list is no longer on the stick.", true);

  const missing = list.rows.filter((r) => r.track === null).length;
  return (
    <section className="flex min-h-0 flex-1 flex-col" aria-label={list.name}>
      <header className="flex shrink-0 flex-wrap items-baseline gap-x-2 gap-y-1 border-b border-border bg-bg-subtle px-3 py-2.5">
        <h2 className="m-0 text-md font-semibold">{list.name}</h2>
        <span className="text-sm text-fg-muted">
          {device === undefined ? open.mountPoint : deviceName(device)} ·{" "}
          {open.kind === "history" ? "history" : "playlist"} · {list.rows.length} track
          {list.rows.length === 1 ? "" : "s"}
          {missing > 0 && ` · ${missing} not on the stick`}
        </span>
      </header>
      <Rows mountPoint={open.mountPoint} rows={list.rows} />
    </section>
  );
}

function Rows({ mountPoint, rows }: { mountPoint: string; rows: StickRow[] }) {
  if (rows.length === 0) return <p className="m-3 text-fg-muted">This list is empty.</p>;
  return (
    <ScrollArea className="min-h-0 flex-1">
      <table
        className="w-full table-fixed border-collapse text-md"
        aria-label="Tracks on the stick"
      >
        <colgroup>
          <col className="w-9" />
          <col className="w-11" />
          <col />
          <col className="w-1/4" />
          <col className="w-1/6" />
          <col className="w-20" />
          <col className="w-14" />
          <col className="w-16" />
        </colgroup>
        <thead>
          <tr>
            <th className={TH}>
              <span className="sr-only">Play</span>
            </th>
            <th className={`${TH} text-right`}>#</th>
            <th className={TH}>Title</th>
            <th className={TH}>Artist</th>
            <th className={TH}>Genre</th>
            <th className={`${TH} text-right`}>BPM</th>
            <th className={TH}>Key</th>
            <th className={`${TH} text-right`}>Time</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) =>
            r.track === null ? (
              <tr
                key={r.n}
                className="text-fg-subtle odd:bg-row-alt"
                title={`Track ${r.trackId} is not on the stick any more`}
              >
                <td className={TD} />
                <td className={`${TD} text-right tabular-nums`}>{r.n}</td>
                <td className={TD} colSpan={6}>
                  Not on the stick
                </td>
              </tr>
            ) : (
              <TrackRow key={r.n} mountPoint={mountPoint} n={r.n} track={r.track} />
            ),
          )}
        </tbody>
      </table>
    </ScrollArea>
  );
}

function TrackRow({
  mountPoint,
  n,
  track: t,
}: {
  mountPoint: string;
  n: number;
  track: StickTrack;
}) {
  const ref = stickRef(mountPoint, t);
  const loaded = usePlayerHas(ref);
  const play = () => void playTrack(ref);
  return (
    <tr
      className="group odd:bg-row-alt data-[loaded]:font-medium data-[loaded]:text-accent"
      data-loaded={loaded || undefined}
      title={t.stickPath}
      onDoubleClick={() => void activateTrack(ref)}
    >
      <td className="px-1">
        <IconButton
          size="xs"
          shape="round"
          className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          aria-label={`Play ${t.title}`}
          onClick={play}
        >
          <PlayIcon />
        </IconButton>
      </td>
      <td className={`${TD} text-right tabular-nums text-fg-muted`}>{n}</td>
      <td className={TD}>{t.title}</td>
      <td className={TD}>{t.artist}</td>
      <td className={TD}>{t.genre}</td>
      <td className={`${TD} text-right tabular-nums`}>{formatBpm(t.bpmX100)}</td>
      <td className={TD}>{t.key === null ? "" : <KeyLabel value={t.key} />}</td>
      <td className={`${TD} text-right tabular-nums`}>{formatDuration(t.durationMs)}</td>
    </tr>
  );
}
