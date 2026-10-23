/**
 * Add filter's search (D35, phase 1 task 8): what a query typed into the
 * field chooser finds. Values come first (a BPM around a number, a star
 * rating, a key in any notation, a genre, a tag with its category), then
 * fields by name. Picking a value sets or extends its row (`addValue`).
 * Pure; no store, no IPC.
 */
import type { Key, KeyInfo, TagTree } from "../lib/ipc";
import { FIELD_GROUPS, FIELD_LABELS, type FieldKey, type RowValue, tagKey } from "./ruleTree";

/** "128" finds BPM 126–130. */
export const BPM_SEARCH_TOLERANCE = 2;
/** Lowest and highest number read as a BPM. */
const BPM_MIN = 40;
const BPM_MAX = 300;
/** Genres and tags listed at most, so a short query doesn't flood the list. */
const MAX_NAMED = 8;

/** Where values come from: the drawer's pickers' options. */
export interface SearchSource {
  genres: readonly string[];
  keys: readonly KeyInfo[];
  tags: TagTree | null;
  keyLabel: (key: Key) => string;
}

export interface FieldEntry {
  key: FieldKey;
  label: string;
}

export interface ValueHit {
  /** Stable within one search, for React keys and `aria-activedescendant`. */
  id: string;
  /** "Genre", "# Situation". */
  field: string;
  /** "is", "has", "around", "rated". */
  verb: string;
  label: string;
  /** Grey text after the label: a tag's category, a BPM range. */
  detail?: string;
  value: RowValue;
}

export interface SearchResult {
  values: ValueHit[];
  fields: FieldEntry[];
}

/** One entry per field, in the chooser's order; each My Tags category is a field. */
export function fieldEntries(
  tags: TagTree | null,
): { label: string; group: "tags" | "plain"; entries: FieldEntry[] }[] {
  return FIELD_GROUPS.map((g) => ({
    label: g.label,
    group: g.fields === "tags" ? "tags" : "plain",
    entries:
      g.fields === "tags"
        ? (tags?.categories ?? []).map((c) => ({ key: tagKey(c.id), label: `# ${c.name}` }))
        : g.fields.map((f) => ({ key: f as FieldKey, label: FIELD_LABELS[f] })),
  }));
}

function bpmHit(q: string): ValueHit[] {
  if (!/^\d{2,3}(\.\d+)?$/.test(q)) return [];
  const n = Number(q);
  if (n < BPM_MIN || n > BPM_MAX) return [];
  const min = n - BPM_SEARCH_TOLERANCE;
  const max = n + BPM_SEARCH_TOLERANCE;
  return [
    {
      id: "bpm",
      field: "BPM",
      verb: "around",
      label: q,
      detail: `${min}–${max}`,
      value: { field: "bpm", min, max },
    },
  ];
}

/** "4*", "4 stars", "★★★★". */
function starsHit(q: string): ValueHit[] {
  const digit = /^([1-5])\s*(\*|★|stars?)$/.exec(q)?.[1];
  const stars = digit !== undefined ? Number(digit) : (/^★{1,5}$/.exec(q)?.[0].length ?? 0);
  if (stars === 0) return [];
  return [
    {
      id: "rating",
      field: "Rating",
      verb: "at least",
      label: "★".repeat(stars),
      value: { field: "rating", op: "atLeast", stars },
    },
  ];
}

/** A key typed exactly, in any notation ("8a", "am", "1m"). */
function keyHits(q: string, src: SearchSource): ValueHit[] {
  return src.keys
    .filter((k) => [k.key, k.camelot, k.openKey].some((s) => s.toLowerCase() === q))
    .map((k) => ({
      id: `key:${k.key}`,
      field: "Key",
      verb: "is",
      label: src.keyLabel(k.key),
      value: { field: "key", keys: [k.key] },
    }));
}

/** Names that start with the query first, then names that contain it. */
function rank<T>(items: readonly T[], name: (t: T) => string, q: string): T[] {
  const hits = items.filter((t) => name(t).toLowerCase().includes(q));
  const starts = (t: T) => (name(t).toLowerCase().startsWith(q) ? 0 : 1);
  return hits.sort((a, b) => starts(a) - starts(b));
}

function namedHits(q: string, src: SearchSource): ValueHit[] {
  const genres = rank(src.genres, (g) => g, q).map(
    (g): ValueHit => ({
      id: `genre:${g}`,
      field: "Genre",
      verb: "is",
      label: g,
      value: { field: "genre", genres: [g] },
    }),
  );
  const tags = (src.tags?.categories ?? []).flatMap((c) =>
    c.tags.map((t) => ({ category: c, tag: t })),
  );
  const tagHits = rank(tags, (x) => x.tag.name, q).map(
    ({ category, tag }): ValueHit => ({
      id: `tag:${tag.id}`,
      field: `# ${category.name}`,
      verb: "has",
      label: tag.name,
      value: { field: "tag", category: category.id, ids: [tag.id], all: false },
    }),
  );
  return [...genres, ...tagHits].slice(0, MAX_NAMED);
}

/** What `query` finds; nothing for a blank one (the chooser then lists every field). */
export function searchFilters(query: string, src: SearchSource): SearchResult {
  const q = query.trim().toLowerCase();
  if (q === "") return { values: [], fields: [] };
  const values = [...bpmHit(q), ...starsHit(q), ...keyHits(q, src), ...namedHits(q, src)];
  const fields = fieldEntries(src.tags)
    .flatMap((g) => g.entries)
    .filter((f) => f.label.toLowerCase().includes(q));
  return { values, fields };
}
