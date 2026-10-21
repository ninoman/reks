/**
 * The value part of a rule row, by field. Reuses the filter bar's pickers
 * (keys, tags, colours) and its inputs; a field with nothing to choose
 * (Missing) shows plain text.
 */
import type { PlaylistNode } from "../lib/ipc";
import type { RowValue } from "../store/ruleTree";
import { ColourPicker } from "./ColourPicker";
import { KeyPicker } from "./KeyPicker";
import { TagFilterPicker } from "./TagFilterPicker";
import { Input } from "./ui/Input";
import { Select } from "./ui/Select";

/** Options the editors draw from (`library_facets` and the playlist tree). */
export interface RuleChoices {
  genres: readonly string[];
  colours: readonly string[];
  playlists: readonly PlaylistNode[];
}

const NBSP = " ";

function number(s: string, integer: boolean): number | null {
  const n = integer ? Number.parseInt(s, 10) : Number.parseFloat(s);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

function RangeInputs({
  label,
  min,
  max,
  integer,
  onChange,
}: {
  label: string;
  min: number | null;
  max: number | null;
  integer: boolean;
  onChange: (min: number | null, max: number | null) => void;
}) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <Input
        size="sm"
        type="number"
        className="w-16"
        aria-label={`Minimum ${label}`}
        placeholder="min"
        min={0}
        value={min ?? ""}
        onChange={(e) => onChange(number(e.target.value, integer), max)}
      />
      –
      <Input
        size="sm"
        type="number"
        className="w-16"
        aria-label={`Maximum ${label}`}
        placeholder="max"
        min={0}
        value={max ?? ""}
        onChange={(e) => onChange(min, number(e.target.value, integer))}
      />
    </span>
  );
}

export function RuleValueEditor({
  value,
  choices,
  onChange,
}: {
  value: RowValue;
  choices: RuleChoices;
  onChange: (value: RowValue) => void;
}) {
  switch (value.field) {
    case null:
      return <span className="text-sm text-fg-subtle">Pick a field first</span>;
    case "tag":
      return (
        <TagFilterPicker
          label="Tags"
          emptyLabel="Choose tags…"
          ids={value.ids}
          onChange={(ids) => onChange({ field: "tag", ids })}
        />
      );
    case "playlist":
      return (
        <Select
          size="sm"
          aria-label="Playlist"
          className="min-w-40"
          value={value.id ?? ""}
          onChange={(e) =>
            onChange({
              field: "playlist",
              id: e.target.value === "" ? null : Number(e.target.value),
            })
          }
        >
          <option value="">Choose a playlist…</option>
          {choices.playlists.map((p) => (
            <option key={p.id} value={p.id}>
              {NBSP.repeat(p.depth * 2)}
              {p.name}
              {p.smart ? " (smart)" : ""}
            </option>
          ))}
        </Select>
      );
    case "genre":
      return (
        <Select
          size="sm"
          aria-label="Genre"
          value={value.genre ?? ""}
          onChange={(e) =>
            onChange({ field: "genre", genre: e.target.value === "" ? null : e.target.value })
          }
        >
          <option value="">Choose a genre…</option>
          {choices.genres.map((g) => (
            <option key={g} value={g}>
              {g}
            </option>
          ))}
        </Select>
      );
    case "key":
      return (
        <KeyPicker
          label="Keys"
          emptyLabel="Choose keys…"
          value={value.keys}
          onChange={(keys) => onChange({ field: "key", keys })}
        />
      );
    case "bpm":
      return (
        <RangeInputs
          label="BPM"
          integer={false}
          min={value.min}
          max={value.max}
          onChange={(min, max) => onChange({ field: "bpm", min, max })}
        />
      );
    case "color":
      return (
        <ColourPicker
          colours={choices.colours}
          value={value.color}
          onChange={(color) => onChange({ field: "color", color })}
        />
      );
    case "rating":
      return (
        <Select
          size="sm"
          aria-label="Rating at least"
          value={value.min}
          onChange={(e) => onChange({ field: "rating", min: Number(e.target.value) })}
        >
          <option value={0}>Choose a rating…</option>
          {[1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>
              {"★".repeat(n)} or more
            </option>
          ))}
        </Select>
      );
    case "plays":
      return (
        <RangeInputs
          label="plays"
          integer
          min={value.min}
          max={value.max}
          onChange={(min, max) => onChange({ field: "plays", min, max })}
        />
      );
    case "lastPlayed":
      return (
        <span className="inline-flex items-center gap-1.5">
          <Select
            size="sm"
            aria-label="Last played"
            value={value.mode}
            onChange={(e) =>
              onChange({ ...value, mode: e.target.value === "never" ? "never" : "since" })
            }
          >
            <option value="never">never played</option>
            <option value="since">played since</option>
          </Select>
          {value.mode === "since" && (
            <Input
              size="sm"
              type="date"
              className="w-36"
              aria-label="Played since"
              value={value.date}
              onChange={(e) => onChange({ ...value, date: e.target.value })}
            />
          )}
        </span>
      );
    case "missing":
      return <span className="text-sm text-fg-muted">missing</span>;
  }
}
