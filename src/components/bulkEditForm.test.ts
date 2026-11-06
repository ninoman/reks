import { describe, expect, it } from "vitest";

import type { TrackRow } from "../lib/ipc";
import {
  buildBulkPatch,
  changeSummary,
  countChanged,
  emptyBulkForm,
  setCommentMode,
  setCommentText,
  setField,
  sharedValue,
  toggleTag,
} from "./bulkEditForm";

function row(id: number, patch: Partial<TrackRow> = {}): TrackRow {
  return {
    id,
    title: `T${id}`,
    artist: "Ana",
    album: null,
    genre: "House",
    label: null,
    remixer: null,
    comment: null,
    key: "Am",
    bpmX100: 12800,
    rating: 3,
    color: null,
    durationMs: 1,
    year: 2001,
    kind: "mp3",
    bitRateKbps: null,
    dateAdded: "2026-01-01",
    playCount: 0,
    lastPlayed: null,
    missing: false,
    needsAnalysis: false,
    artworkId: null,
    entryPosition: null,
    tags: [],
    hotCues: [],
    ...patch,
  };
}

const rows = [row(1), row(2, { artist: "Bo", year: null, tags: [5] }), row(3, { tags: [5, 6] })];

describe("sharedValue", () => {
  it("is the common value, or mixed when the tracks differ", () => {
    expect(sharedValue(rows, "genre")).toEqual({ kind: "same", value: "House" });
    expect(sharedValue(rows, "artist")).toEqual({ kind: "mixed" });
    expect(sharedValue(rows, "year")).toEqual({ kind: "mixed" });
    expect(sharedValue(rows, "album")).toEqual({ kind: "same", value: "" });
  });
});

describe("bulk form", () => {
  it("sends nothing until a field is touched", () => {
    expect(buildBulkPatch(emptyBulkForm)).toEqual({});
    expect(countChanged(rows, emptyBulkForm)).toBe(0);
  });

  it("typing back the shared value is untouched again", () => {
    const edited = setField(rows, emptyBulkForm, "genre", "Techno");
    expect(buildBulkPatch(edited)).toEqual({ genre: "Techno" });
    expect(buildBulkPatch(setField(rows, edited, "genre", "House"))).toEqual({});
    // A mixed field has no value to type back to.
    expect(buildBulkPatch(setField(rows, emptyBulkForm, "artist", "Ana"))).toEqual({
      artist: "Ana",
    });
  });

  it("empty input clears; rating and year are numbers", () => {
    let f = setField(rows, emptyBulkForm, "genre", "");
    f = setField(rows, f, "year", "1999");
    f = setField(rows, f, "rating", "0");
    f = setField(rows, f, "color", "red");
    expect(buildBulkPatch(f)).toEqual({ genre: null, year: 1999, rating: 0, color: "red" });
    expect(buildBulkPatch(setField(rows, emptyBulkForm, "year", ""))).toEqual({ year: null });
  });

  it("counts only the tracks a change touches", () => {
    // Genre: all three already House.
    expect(countChanged(rows, setField(rows, emptyBulkForm, "genre", "House "))).toBe(0);
    expect(countChanged(rows, setField(rows, emptyBulkForm, "genre", "Techno"))).toBe(3);
    // Year 2001 is on two of the three tracks.
    expect(countChanged(rows, setField(rows, emptyBulkForm, "year", "2001"))).toBe(1);
    // The typed key is compared in the shown notation.
    const label = (k: string) => (k === "Am" ? "8A" : k);
    expect(countChanged(rows, setField(rows, emptyBulkForm, "key", "8A"), label)).toBe(0);
  });

  it("comment: replace and append are separate, append needs text", () => {
    const typed = setCommentText(emptyBulkForm, "new");
    expect(buildBulkPatch(typed).comment).toEqual({ mode: "replace", text: "new" });
    expect(buildBulkPatch(setCommentText(emptyBulkForm, "")).comment).toEqual({
      mode: "replace",
      text: null,
    });
    const appended = setCommentMode(typed, "append");
    expect(buildBulkPatch(appended).comment).toEqual({ mode: "append", text: "new" });
    expect(buildBulkPatch(setCommentText(appended, "  ")).comment).toBeUndefined();
    // Mode alone, with nothing typed, edits nothing.
    expect(buildBulkPatch(setCommentMode(emptyBulkForm, "append"))).toEqual({});
    expect(countChanged(rows, appended)).toBe(3);
  });

  it("tags: add to all or remove from all, a second click undoes", () => {
    // Tag 5 is on 2 of 3 tracks: a click adds it; tag 7 on none: adds.
    let f = toggleTag(emptyBulkForm, 5, 3, 2);
    expect(buildBulkPatch(f)).toEqual({ addTags: [5] });
    expect(countChanged(rows, f)).toBe(1);
    f = toggleTag(f, 5, 3, 2);
    expect(buildBulkPatch(f)).toEqual({});
    // A tag all three have is removed.
    f = toggleTag(emptyBulkForm, 5, 2, 2);
    f = toggleTag(f, 6, 3, 1);
    expect(buildBulkPatch(f)).toEqual({ removeTags: [5], addTags: [6] });
    expect(changeSummary(f)).toEqual(["Add 1 tag(s)", "Remove 1 tag(s)"]);
  });
});
