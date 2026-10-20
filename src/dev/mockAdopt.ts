/**
 * Dev-only adopt dry run for the mock backend (`mockBackend.ts`): the GIGS
 * stick holds a rekordbox library with three playlists, one exactly like a
 * library playlist (Link offered) and one with a track the library lacks.
 */

import type { AdoptChoices, AdoptEntry, AdoptPlan, AdoptReport, AdoptScan } from "../lib/ipc";

function entry(title: string, trackId: number | null): AdoptEntry {
  return {
    stickPath: `/Contents/Artist/Album/${title}.mp3`,
    title,
    artist: "Artist",
    kind: "track",
    trackId,
    matchedBy: trackId === null ? null : "pathSuffix",
  };
}

const PLAN: AdoptPlan = {
  volumeSerial: "DISK5S1-0001",
  writtenByReks: false,
  history: null,
  historySessions: 1,
  historyText:
    "1 play history session is imported first (mark practice sessions in History afterwards).",
  playlists: [
    {
      pdbId: 12,
      path: ["Sets", "Warmup"],
      pathText: "Sets / Warmup",
      entries: [entry("Track 1", 1), entry("Track 2", 2), entry("Track 3", 3)],
      matched: 3,
      unmatched: 0,
      dividers: 0,
      link: [{ playlistId: 2, pathText: "Sets / Warmup" }],
      defaultChoice: { kind: "create" },
      text: "3 tracks; the same playlist is in your library",
    },
    {
      pdbId: 11,
      path: ["Sets", "Peak"],
      pathText: "Sets / Peak",
      entries: [entry("Track 5", 5), entry("Track 4", 4)],
      matched: 2,
      unmatched: 0,
      dividers: 0,
      link: [],
      defaultChoice: { kind: "create" },
      text: "2 tracks",
    },
    {
      pdbId: 13,
      path: ["Odd"],
      pathText: "Odd",
      entries: [entry("Track 6", 6), entry("Bootleg", null)],
      matched: 1,
      unmatched: 1,
      dividers: 0,
      link: [],
      defaultChoice: { kind: "create" },
      text: "2 tracks, 1 not in your library",
    },
  ],
  unmatched: [
    { stickPath: "/Contents/Artist/Album/Bootleg.mp3", title: "Bootleg", artist: "Artist" },
  ],
  summary: "3 playlists, 7 track entries: 6 in your library, 1 not",
  unmatchedText:
    "1 track is not in your library: removed from the stick on the first reks sync unless you add the files to your library.",
  notes: [],
};

let lastId = 0;

export function mockAdopt(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "adopt_scan":
      lastId += 1;
      return { planId: lastId, plan: PLAN } satisfies AdoptScan;
    case "adopt_apply": {
      if (args.planId !== lastId) throw new Error("this dry run is out of date; scan again");
      const choices = args.choices as AdoptChoices;
      const kinds = PLAN.playlists.map((p) => (choices.playlists[p.pdbId] ?? p.defaultChoice).kind);
      const count = (k: string) => kinds.filter((x) => x === k).length;
      return {
        history: { created: [101], extended: [], skipped: 0, entries: 2, unmatched: 0 },
        folderId: 900,
        created: Array.from({ length: count("create") }, (_, i) => 901 + i),
        linked: count("link") > 0 ? [2] : [],
        skipped: count("skip"),
        unmatched: PLAN.unmatched,
        summary: `${count("create")} created, ${count("link")} linked, ${count("skip")} skipped; 1 play history session imported.`,
      } satisfies AdoptReport;
    }
    default:
      return undefined;
  }
}
