/** Track table columns: header, width, server sort column and cell renderer. */
import type { ReactNode } from "react";

import { formatBpm, formatDuration, formatRating } from "../lib/format";
import type { SortColumn, TrackRow } from "../lib/ipc";
import { KeyLabel } from "./KeyLabel";

export interface ColumnSpec {
  id: string;
  header: string;
  width: number;
  /** `null` = not sortable. */
  sort: SortColumn | null;
  align?: "right";
  cell: (row: TrackRow) => ReactNode;
}

const text = (v: string | number | null) => v ?? "";

export const COLUMNS: ColumnSpec[] = [
  {
    id: "status",
    header: "",
    width: 28,
    sort: null,
    cell: (r) =>
      r.missing ? (
        <span className="status missing" title="File missing">
          !
        </span>
      ) : r.needsAnalysis ? (
        <span className="status needs-analysis" title="Needs analysis (no beatgrid)">
          ○
        </span>
      ) : null,
  },
  {
    id: "position",
    header: "#",
    width: 44,
    sort: "playlistPosition",
    align: "right",
    cell: (r) => (r.entryPosition === null ? "" : r.entryPosition + 1),
  },
  { id: "title", header: "Title", width: 280, sort: "title", cell: (r) => text(r.title) },
  { id: "artist", header: "Artist", width: 200, sort: "artist", cell: (r) => text(r.artist) },
  { id: "album", header: "Album", width: 180, sort: "album", cell: (r) => text(r.album) },
  { id: "genre", header: "Genre", width: 120, sort: "genre", cell: (r) => text(r.genre) },
  { id: "label", header: "Label", width: 140, sort: "label", cell: (r) => text(r.label) },
  { id: "remixer", header: "Remixer", width: 140, sort: null, cell: (r) => text(r.remixer) },
  {
    id: "bpm",
    header: "BPM",
    width: 70,
    sort: "bpm",
    align: "right",
    cell: (r) => formatBpm(r.bpmX100),
  },
  {
    id: "key",
    header: "Key",
    width: 56,
    sort: "key",
    cell: (r) => (r.key === null ? "" : <KeyLabel value={r.key} />),
  },
  {
    id: "rating",
    header: "Rating",
    width: 76,
    sort: "rating",
    cell: (r) => <span className="rating">{formatRating(r.rating)}</span>,
  },
  {
    id: "color",
    header: "Colour",
    width: 60,
    sort: "color",
    cell: (r) =>
      r.color === null ? null : (
        <span
          className={`swatch swatch-${r.color}`}
          role="img"
          title={r.color}
          aria-label={r.color}
        />
      ),
  },
  {
    id: "duration",
    header: "Time",
    width: 60,
    sort: "duration",
    align: "right",
    cell: (r) => formatDuration(r.durationMs),
  },
  {
    id: "year",
    header: "Year",
    width: 56,
    sort: "year",
    align: "right",
    cell: (r) => text(r.year),
  },
  { id: "kind", header: "Kind", width: 56, sort: "kind", cell: (r) => r.kind },
  {
    id: "bitRate",
    header: "Bit rate",
    width: 70,
    sort: "bitRate",
    align: "right",
    cell: (r) => text(r.bitRateKbps),
  },
  {
    id: "dateAdded",
    header: "Added",
    width: 96,
    sort: "dateAdded",
    cell: (r) => r.dateAdded.slice(0, 10),
  },
  {
    id: "playCount",
    header: "Plays",
    width: 56,
    sort: "playCount",
    align: "right",
    cell: (r) => r.playCount,
  },
  { id: "comment", header: "Comment", width: 200, sort: null, cell: (r) => text(r.comment) },
];

export const COLUMN_BY_ID = new Map(COLUMNS.map((c) => [c.id, c]));

/** Hidden until switched on in the column chooser. */
export const DEFAULT_HIDDEN = [
  "album",
  "label",
  "remixer",
  "year",
  "kind",
  "bitRate",
  "playCount",
  "comment",
];
