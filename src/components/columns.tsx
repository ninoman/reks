/** Track table columns: header, width, server sort column and cell renderer. */
import type { ReactNode } from "react";

import { formatBpm, formatDuration, formatRating } from "../lib/format";
import type { SortColumn, TrackRow } from "../lib/ipc";
import { tagIndex } from "../store/tagModel";
import { useTags } from "../store/tags";
import { KeyLabel } from "./KeyLabel";
import { Artwork } from "./ui/Artwork";
import { Swatch } from "./ui/Swatch";

export interface ColumnSpec {
  id: string;
  header: string;
  width: number;
  /** `null` = not sortable. */
  sort: SortColumn | null;
  align?: "right";
  /** Takes a share of spare width when the table is narrower than its container. */
  grow?: true;
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
        <span className="font-semibold text-danger" title="File missing">
          !
        </span>
      ) : r.needsAnalysis ? (
        <span className="text-fg-subtle" title="Needs analysis (no beatgrid)">
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
  {
    id: "title",
    grow: true,
    header: "Title",
    width: 280,
    sort: "title",
    cell: (r) => (
      <span className="flex items-center gap-1.5">
        <Artwork size="row" artworkId={r.artworkId} />
        <span className="truncate">{text(r.title)}</span>
      </span>
    ),
  },
  {
    id: "artist",
    grow: true,
    header: "Artist",
    width: 200,
    sort: "artist",
    cell: (r) => text(r.artist),
  },
  {
    id: "album",
    grow: true,
    header: "Album",
    width: 180,
    sort: "album",
    cell: (r) => text(r.album),
  },
  {
    id: "genre",
    grow: true,
    header: "Genre",
    width: 120,
    sort: "genre",
    cell: (r) => text(r.genre),
  },
  {
    id: "label",
    grow: true,
    header: "Label",
    width: 140,
    sort: "label",
    cell: (r) => text(r.label),
  },
  {
    id: "remixer",
    grow: true,
    header: "Remixer",
    width: 140,
    sort: null,
    cell: (r) => text(r.remixer),
  },
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
    cell: (r) => <span className="tracking-[-1px] text-rating">{formatRating(r.rating)}</span>,
  },
  {
    id: "color",
    header: "Colour",
    width: 60,
    sort: "color",
    cell: (r) =>
      r.color === null ? null : (
        <Swatch color={r.color} role="img" title={r.color} aria-label={r.color} />
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
  {
    id: "lastPlayed",
    header: "Last played",
    width: 96,
    sort: "lastPlayed",
    cell: (r) => text(r.lastPlayed?.slice(0, 10) ?? null),
  },
  {
    id: "tags",
    header: "My Tags",
    width: 220,
    sort: null,
    // Read, not subscribed: the table re-renders when the tree changes (TrackTable).
    cell: (r) => {
      const tree = useTags.getState().tree;
      if (tree === null || r.tags.length === 0) return "";
      const names = tagIndex(tree);
      return r.tags
        .map((id) => names.get(id)?.tag.name)
        .filter((n) => n !== undefined)
        .join(", ");
    },
  },
  {
    id: "comment",
    grow: true,
    header: "Comment",
    width: 200,
    sort: null,
    cell: (r) => text(r.comment),
  },
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
  "lastPlayed",
  "comment",
  "tags",
];
