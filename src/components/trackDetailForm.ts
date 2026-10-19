/**
 * Edit form model for the detail panel: the editable fields, the form's
 * string values, and the patch of changed fields sent to
 * `library_update_track`. Values are passed through as typed; validation
 * (ranges, colours, keys, control characters) happens in `reks-core`.
 */
import type { Track, TrackPatch } from "../lib/ipc";

type TextField =
  | "title"
  | "artist"
  | "album"
  | "albumArtist"
  | "genre"
  | "composer"
  | "remixer"
  | "label"
  | "mix"
  | "grouping"
  | "comment";
type NumberField = "trackNo" | "discNo" | "year";
export type EditField = TextField | NumberField | "rating" | "color" | "key";

export const TEXT_FIELDS: { id: TextField; label: string; multiline?: boolean }[] = [
  { id: "title", label: "Title" },
  { id: "artist", label: "Artist" },
  { id: "album", label: "Album" },
  { id: "albumArtist", label: "Album artist" },
  { id: "genre", label: "Genre" },
  { id: "composer", label: "Composer" },
  { id: "remixer", label: "Remixer" },
  { id: "label", label: "Label" },
  { id: "mix", label: "Mix" },
  { id: "grouping", label: "Grouping" },
  { id: "comment", label: "Comment", multiline: true },
];

export const NUMBER_FIELDS: { id: NumberField; label: string }[] = [
  { id: "trackNo", label: "Track #" },
  { id: "discNo", label: "Disc #" },
  { id: "year", label: "Year" },
];

/** Every editable field as the string its input shows; `""` = empty. */
export type EditForm = Record<EditField, string>;

const ALL_FIELDS: EditField[] = [
  ...TEXT_FIELDS.map((f) => f.id),
  ...NUMBER_FIELDS.map((f) => f.id),
  "rating",
  "color",
  "key",
];

export function toForm(t: Track): EditForm {
  const form = {} as EditForm;
  for (const f of ALL_FIELDS) {
    const v = t[f];
    form[f] = v === null ? "" : String(v);
  }
  return form;
}

/** Only the fields that differ from `initial`; empty input clears (`null`). */
export function buildPatch(initial: EditForm, form: EditForm): TrackPatch {
  const patch: TrackPatch = {};
  const p = patch as Record<string, string | number | null>;
  for (const f of ALL_FIELDS) {
    const v = form[f];
    if (v === initial[f]) continue;
    if (f === "rating") p[f] = Number(v);
    else if (NUMBER_FIELDS.some((n) => n.id === f)) p[f] = v.trim() === "" ? null : Number(v);
    else p[f] = v === "" ? null : v;
  }
  return patch;
}
