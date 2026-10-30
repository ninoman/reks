/** Tooltip and suggested-name text for a draft. */
import type { Key, TagId } from "../../lib/ipc";
import type { GroupDraft, ItemDraft, RatingOp, RowDraft } from "./model";
import { isRowActive } from "./toRule";

// ---- summary (tooltips only: the drawer's rows say it better) --------------

/** Names the summary needs from the rest of the app. */
export interface SummaryNames {
  tag(id: TagId): string;
  category(id: TagId): string;
  playlist(id: number): string;
  key(key: Key): string;
}

function range(min: number | null, max: number | null): string {
  if (min !== null && max !== null) return `${min}–${max}`;
  return min !== null ? `${min}+` : `up to ${max}`;
}

/** "A", "A or B", "A, B or C". */
function joinList(items: string[], word: "or" | "and"): string {
  return items.length < 2
    ? items.join("")
    : `${items.slice(0, -1).join(", ")} ${word} ${items.at(-1)}`;
}

const RATING_TEXT: Record<RatingOp, (n: number) => string> = {
  atLeast: (n) => `${n}★ or more`,
  exactly: (n) => `${n}★`,
  atMost: (n) => `up to ${n}★`,
};

/** One row that filters as a phrase; `null` for one that doesn't. */
export function rowText(row: RowDraft, names: SummaryNames): string | null {
  if (!isRowActive(row)) return null;
  const v = row.value;
  const not = row.not ? "not " : "";
  switch (v.field) {
    case null:
      return null;
    case "genre":
      return `genre ${not}${joinList(v.genres, "or")}`;
    case "color":
      return `colour ${not}${joinList(v.colours, "or")}`;
    case "playlist":
      return `${not}in playlist ${joinList(v.ids.map(names.playlist), "or")}`;
    case "key":
      return `key ${not}${joinList(v.keys.map(names.key), "or")}`;
    case "tag": {
      const cat = v.category === null ? "tag" : names.category(v.category);
      return `${cat} ${not}${joinList(v.ids.map(names.tag), v.all ? "and" : "or")}`;
    }
    case "bpm":
      return `BPM ${not}${range(v.min, v.max)}`;
    case "plays":
      return `plays ${not}${range(v.min, v.max)}`;
    case "rating":
      return `rating ${not}${RATING_TEXT[v.op](v.stars)}`;
    case "lastPlayed":
      if (v.mode === "never") return row.not ? "has been played" : "never played";
      return `${not}played since ${v.date}`;
    case "missing":
      return `file ${(v.missing ?? true) !== row.not ? "missing" : "present"}`;
  }
}

function itemText(item: ItemDraft, names: SummaryNames, nested: boolean): string | null {
  if (item.kind === "row") return rowText(item, names);
  const parts = item.rules.flatMap((r) => itemText(r, names, true) ?? []);
  if (parts.length === 0) return null;
  const text = parts.join(item.mode === "all" ? " · " : " or ");
  if (item.mode === "none") return `none of (${text})`;
  return nested && parts.length > 1 ? `(${text})` : text;
}

/** "BPM 124–130 · key 8A or 9A · Situation not Warm-up"; `null` while nothing filters. */
export function summarize(root: GroupDraft, names: SummaryNames): string | null {
  const text = itemText(root, names, false);
  return text === null ? null : text.charAt(0).toUpperCase() + text.slice(1);
}

/** Longest name Save… suggests. */
const NAME_MAX = 40;

/** A row's values in a few words, for a suggested name ("Techno", "124–130 BPM"). */
function rowWords(row: RowDraft, names: SummaryNames): string | null {
  if (!isRowActive(row)) return null;
  const v = row.value;
  const not = row.not ? "not " : "";
  switch (v.field) {
    case null:
      return null;
    case "genre":
      return not + v.genres.join("/");
    case "color":
      return not + v.colours.join("/");
    case "playlist":
      return `${row.not ? "not in" : "in"} ${v.ids.map(names.playlist).join("/")}`;
    case "key":
      return not + v.keys.map(names.key).join("/");
    case "tag":
      return not + v.ids.map(names.tag).join(v.all ? " + " : "/");
    case "bpm":
      return `${not}${range(v.min, v.max)} BPM`;
    case "plays":
      return `${not}${range(v.min, v.max)} plays`;
    case "rating":
      return not + RATING_TEXT[v.op](v.stars);
    case "lastPlayed":
      if (v.mode === "never") return row.not ? "Played" : "Unplayed";
      return `${not}played since ${v.date}`;
    case "missing":
      return (v.missing ?? true) !== row.not ? "Missing files" : "Present files";
  }
}

/**
 * A name for Save… from the rows that filter, in drawer order: "Unplayed,
 * 124–130 BPM". Cut at a comma to fit; "My filter" when nothing filters.
 */
export function suggestName(root: GroupDraft, names: SummaryNames): string {
  const words: string[] = [];
  const walk = (g: GroupDraft) => {
    for (const i of g.rules) {
      if (i.kind === "group") walk(i);
      else {
        const w = rowWords(i, names);
        if (w !== null && w !== "") words.push(w);
      }
    }
  };
  walk(root);
  let name = "";
  for (const w of words) {
    const next = name === "" ? w : `${name}, ${w}`;
    if (next.length > NAME_MAX) break;
    name = next;
  }
  if (name === "") name = words[0]?.slice(0, NAME_MAX) ?? "";
  return name === "" ? "My filter" : name.charAt(0).toUpperCase() + name.slice(1);
}
