/**
 * The value part of a Filters drawer row, by field. The operator lives in
 * the picker: Any of / None of for lists (None of is the row's Exclude),
 * also All of for a tag category, at least / exactly / at most for Rating,
 * never / since a date for Last played. Ranges are two inputs in the row,
 * either side optional. Presentational: the row passes the draft and takes
 * the patch back.
 */
import { type ReactNode, useEffect, useRef } from "react";

import type { Key, KeyInfo, PlaylistNode, TagTree } from "../lib/ipc";
import type { RatingOp, RowDraft, RowValue } from "../store/ruleTree";
import { KeyGrid } from "./KeyGrid";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Swatch } from "./ui/Swatch";
import { ListPicker, type OptionsStatus, PickerPopover } from "./ValuePicker";

/** What the editors draw from (`library_facets`, the playlist tree, My Tags). */
export interface RuleChoices {
  genres: readonly string[];
  colours: readonly string[];
  keys: readonly KeyInfo[];
  /** Genres, colours and keys. */
  facets: OptionsStatus;
  playlists: readonly PlaylistNode[];
  tags: TagTree | null;
  tagsStatus: OptionsStatus;
  keyLabel: (key: Key) => string;
}

export interface RowPatch {
  value?: RowValue;
  not?: boolean;
}

const NBSP = " ";

const LIST_MODES = [
  { value: "any", label: "Any of" },
  { value: "none", label: "None of" },
] as const;

const TAG_MODES = [
  { value: "any", label: "Any of" },
  { value: "all", label: "All of" },
  { value: "none", label: "None of" },
] as const;

const RATING_MODES = [
  { value: "atLeast", label: "At least" },
  { value: "exactly", label: "Exactly" },
  { value: "atMost", label: "At most" },
] as const;

const LAST_PLAYED_MODES = [
  { value: "never", label: "Never" },
  { value: "since", label: "Since a date" },
] as const;

function list(items: readonly string[]): string | null {
  return items.length === 0 ? null : items.join(", ");
}

function stars(n: number): string {
  return "★".repeat(n);
}

const RATING_SUMMARY: Record<RatingOp, (n: number) => string> = {
  atLeast: (n) => `${stars(n)} +`,
  exactly: (n) => stars(n),
  atMost: (n) => `≤ ${stars(n)}`,
};

function number(s: string, integer: boolean): number | null {
  const n = integer ? Number.parseInt(s, 10) : Number.parseFloat(s);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

function RangeInputs({
  label,
  min,
  max,
  integer,
  focus,
  onFocused,
  onChange,
}: {
  label: string;
  min: number | null;
  max: number | null;
  integer: boolean;
  /** Take focus on the minimum (Enter on the row, or a row just added). */
  focus: boolean;
  onFocused: () => void;
  onChange: (min: number | null, max: number | null) => void;
}) {
  const minRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!focus) return;
    minRef.current?.focus();
    onFocused();
  }, [focus, onFocused]);
  return (
    <span className="inline-flex items-center gap-1">
      <Input
        ref={minRef}
        size="sm"
        type="number"
        className="w-14"
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
        className="w-14"
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
  label,
  row,
  choices,
  onChange,
  open,
  onOpenChange,
}: {
  /** The field's name ("Genre", "Situation"). */
  label: string;
  row: RowDraft;
  choices: RuleChoices;
  onChange: (patch: RowPatch) => void;
  /** The picker is open (or, for a range, its minimum takes focus). */
  open: boolean;
  onOpenChange: (open: boolean) => void;
}): ReactNode {
  const v = row.value;
  const listMode = row.not ? "none" : "any";
  const onListMode = (m: string) => onChange({ not: m === "none" });
  const popover = { label, open, onOpenChange };
  const closed = () => onOpenChange(false);

  switch (v.field) {
    case null:
      return null;
    case "genre":
      return (
        <ListPicker
          {...popover}
          summary={list(v.genres)}
          modes={LIST_MODES}
          mode={listMode}
          onModeChange={onListMode}
          noun="genres"
          status={choices.facets}
          options={choices.genres.map((g) => ({ value: g, label: g }))}
          selected={v.genres}
          onChange={(genres) => onChange({ value: { ...v, genres } })}
        />
      );
    case "color":
      return (
        <ListPicker
          {...popover}
          summary={list(v.colours)}
          modes={LIST_MODES}
          mode={listMode}
          onModeChange={onListMode}
          noun="colours"
          status={choices.facets}
          options={choices.colours.map((c) => ({
            value: c,
            label: c,
            lead: <Swatch color={c} size="md" />,
          }))}
          selected={v.colours}
          onChange={(colours) => onChange({ value: { ...v, colours } })}
        />
      );
    case "playlist": {
      const names = new Map(choices.playlists.map((p) => [p.id, p.name]));
      return (
        <ListPicker
          {...popover}
          summary={list(v.ids.map((id) => names.get(id) ?? `Playlist ${id}`))}
          modes={LIST_MODES}
          mode={listMode}
          onModeChange={onListMode}
          noun="playlists"
          status={{ state: "ready" }}
          options={choices.playlists.map((p) => ({
            value: p.id,
            label: `${NBSP.repeat(p.depth * 2)}${p.name}${p.smart ? " (smart)" : ""}`,
          }))}
          selected={v.ids}
          onChange={(ids) => onChange({ value: { ...v, ids } })}
        />
      );
    }
    case "key":
      return (
        <PickerPopover
          {...popover}
          summary={list(v.keys.map(choices.keyLabel))}
          modes={LIST_MODES}
          mode={listMode}
          onModeChange={onListMode}
        >
          {choices.facets.state === "ready" ? (
            <KeyGrid
              keys={choices.keys}
              value={v.keys}
              label={choices.keyLabel}
              onChange={(keys) => onChange({ value: { ...v, keys } })}
            />
          ) : choices.facets.state === "loading" ? (
            <p className="m-0 px-1.5 py-1 text-sm text-fg-muted">Loading…</p>
          ) : (
            <div className="flex flex-col gap-1.5 px-1.5 py-1 text-sm" role="alert">
              <span className="text-danger">Couldn't load keys: {choices.facets.error}</span>
              <Button size="sm" className="self-start" onClick={choices.facets.retry}>
                Retry
              </Button>
            </div>
          )}
        </PickerPopover>
      );
    case "tag": {
      const tree = choices.tags;
      const category = tree?.categories.find((c) => c.id === v.category);
      const options =
        category !== undefined
          ? category.tags.map((t) => ({ value: t.id, label: t.name }))
          : (tree?.categories ?? []).flatMap((c) =>
              c.tags.map((t) => ({ value: t.id, label: `${c.name} · ${t.name}` })),
            );
      const names = new Map(tree?.categories.flatMap((c) => c.tags.map((t) => [t.id, t.name])));
      return (
        <ListPicker
          {...popover}
          summary={list(v.ids.map((id) => names.get(id) ?? `Tag ${id}`))}
          modes={TAG_MODES}
          mode={row.not ? "none" : v.all ? "all" : "any"}
          onModeChange={(m) => onChange({ value: { ...v, all: m === "all" }, not: m === "none" })}
          noun="tags"
          status={tree === null ? choices.tagsStatus : { state: "ready" }}
          empty={`No tags in ${label} yet.`}
          options={options}
          selected={v.ids}
          onChange={(ids) => onChange({ value: { ...v, ids } })}
        />
      );
    }
    case "bpm":
    case "plays":
      return (
        <RangeInputs
          label={label}
          integer={v.field === "plays"}
          min={v.min}
          max={v.max}
          focus={open}
          onFocused={closed}
          onChange={(min, max) => onChange({ value: { ...v, min, max } })}
        />
      );
    case "rating":
      return (
        <PickerPopover
          {...popover}
          summary={v.stars === 0 ? null : RATING_SUMMARY[v.op](v.stars)}
          modes={RATING_MODES}
          mode={v.op}
          onModeChange={(op) => onChange({ value: { ...v, op } })}
        >
          <fieldset className="m-0 flex flex-col border-0 p-0" aria-label="Stars">
            {[1, 2, 3, 4, 5].map((n) => (
              <Button
                key={n}
                size="sm"
                variant="ghost"
                className="justify-start text-rating"
                aria-pressed={v.stars === n}
                aria-label={`${n} ${n === 1 ? "star" : "stars"}`}
                onClick={() => onChange({ value: { ...v, stars: v.stars === n ? 0 : n } })}
              >
                {stars(n)}
              </Button>
            ))}
          </fieldset>
        </PickerPopover>
      );
    case "lastPlayed":
      return (
        <PickerPopover
          {...popover}
          summary={
            v.mode === "never" ? "Never" : v.mode === "since" ? `Since ${v.date || "…"}` : null
          }
          modes={LAST_PLAYED_MODES}
          mode={v.mode}
          onModeChange={(mode) => onChange({ value: { ...v, mode } })}
        >
          {v.mode === "since" ? (
            <Input
              size="sm"
              type="date"
              aria-label="Played since"
              value={v.date}
              onChange={(e) => onChange({ value: { ...v, date: e.target.value } })}
            />
          ) : (
            <p className="m-0 px-1.5 py-1 text-xs text-fg-muted">
              {v.mode === "never"
                ? "Tracks with no plays. Exclude them to keep the played ones."
                : "Never played, or played since a date."}
            </p>
          )}
        </PickerPopover>
      );
    case "missing":
      return (
        <SegmentedControl
          size="xs"
          aria-label={label}
          value={v.missing === null ? "" : v.missing ? "missing" : "present"}
          onValueChange={(m) => onChange({ value: { ...v, missing: m === "missing" } })}
        >
          <Segment value="missing">Missing</Segment>
          <Segment value="present">Present</Segment>
        </SegmentedControl>
      );
  }
}
