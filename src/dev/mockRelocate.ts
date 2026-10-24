/**
 * Dev-only relocate and library-folder dry runs for the mock backend
 * (`mockBackend.ts`): one sure match, identical copies, a look-alike and a
 * track that is not there.
 */

import type {
  FoundFile,
  RelocateChoices,
  RelocatePlan,
  RelocateReport,
  RelocateScan,
  RelocateTrack,
  RemapPlan,
  RemapReport,
} from "../lib/ipc";

const FOLDER = "/Volumes/Archive/Music";

const track = (id: number, label: string): RelocateTrack => ({
  id,
  path: `/Users/dj/Music/old/${label}.flac`,
  label,
});

const file = (path: string, sameSize = true): FoundFile => ({
  path,
  size: 31_200_000,
  durationMs: 245_000,
  sameSize,
});

const PLAN: RelocatePlan = {
  folder: FOLDER,
  insideRoot: false,
  lookedFor: 4,
  filesSeen: 1204,
  sure: [
    {
      track: track(11, "Artist – Moved Track"),
      file: file(`${FOLDER}/House/moved-track.flac`),
    },
  ],
  review: [
    {
      track: track(12, "Artist – Two Copies"),
      basis: "sameContent",
      candidates: [file(`${FOLDER}/A/two-copies.flac`), file(`${FOLDER}/B/two-copies.flac`)],
    },
    {
      track: track(13, "Artist – Look Alike"),
      basis: "possible",
      candidates: [file(`${FOLDER}/Edits/look-alike.flac`, false)],
    },
  ],
  notFound: [track(14, "Artist – Gone")],
  problems: [],
  cancelled: false,
};

const REMAP: RemapPlan = {
  rootId: 1,
  oldPath: "/Users/dj/Music",
  newPath: FOLDER,
  tracks: 2812,
  nestedRoots: 0,
  examples: [
    {
      from: "/Users/dj/Music/House/a.flac",
      to: `${FOLDER}/House/a.flac`,
    },
    {
      from: "/Users/dj/Music/Techno/b.mp3",
      to: `${FOLDER}/Techno/b.mp3`,
    },
  ],
  sampled: 24,
  sampleFound: 24,
  collisions: 0,
};

let lastId = 0;

export function mockRelocate(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "relocate_plan":
      lastId += 1;
      return { planId: lastId, plan: PLAN } satisfies RelocateScan;
    case "relocate_apply": {
      if (args.planId !== lastId) throw new Error("this dry run is out of date; run it again");
      const choices = args.choices as RelocateChoices;
      return {
        relinked: PLAN.sure.length + Object.keys(choices.picks).length,
        skipped: [],
        root: null,
        cancelled: false,
      } satisfies RelocateReport;
    }
    case "relocate_cancel":
      return null;
    case "remap_plan":
      return REMAP;
    case "remap_apply":
      return { tracks: REMAP.tracks, roots: 1 } satisfies RemapReport;
    default:
      return undefined;
  }
}
