/**
 * Dev-only play history for the mock backend (`mockBackend.ts`): a few
 * imported sessions, a stick with new ones to import, and play counts
 * derived as in reks-core (baseline plus included sessions, D23).
 */

import type {
  DeviceInfo,
  HistoryImportPlan,
  HistoryImportReport,
  HistoryScan,
  ImportChoices,
  PlannedSession,
  ScanSource,
  SessionDetail,
  SessionEntryRow,
  SessionPatch,
  SessionQuery,
  SessionRef,
  SessionRow,
  TrackRow,
} from "../lib/ipc";

interface MockEntry {
  kind: "track" | "divider";
  trackId: number | null;
  rawTitle: string | null;
  rawArtist: string | null;
  rawPath: string | null;
  rawDurationMs: number | null;
}

interface MockSession {
  id: number;
  name: string;
  sourceName: string;
  playedOn: string | null;
  venue: string | null;
  notes: string | null;
  excluded: boolean;
  excludedReason: "user" | "pre_baseline" | null;
  importedAt: string;
  entries: MockEntry[];
}

const BASELINE_DATE = "2026-03-01";

export function mockHistory(tracks: TrackRow[]) {
  const baseline = new Map(tracks.map((t) => [t.id, { count: t.playCount, last: t.lastPlayed }]));
  let nextId = 1;
  let sessions: MockSession[] = [];
  let lastPlan: HistoryImportPlan | null = null;
  let planId = 0;

  const track = (id: number) => tracks[id - 1] as TrackRow;
  const entry = (id: number): MockEntry => {
    const t = track(id);
    return {
      kind: "track",
      trackId: id,
      rawTitle: t.title,
      rawArtist: t.artist,
      rawPath: `/Contents/${t.artist}/${t.title}.${t.kind}`,
      rawDurationMs: t.durationMs,
    };
  };
  const unmatched = (title: string, artist: string): MockEntry => ({
    kind: "track",
    trackId: null,
    rawTitle: title,
    rawArtist: artist,
    rawPath: `/Contents/${artist}/${title}.mp3`,
    rawDurationMs: 312_000,
  });
  const divider = (label: string): MockEntry => ({
    kind: "divider",
    trackId: null,
    rawTitle: label,
    rawArtist: null,
    rawPath: "/Contents/Unknown Artist/Unknown Album/div-4-0.wav",
    rawDurationMs: 4000,
  });
  const range = (from: number, n: number) => Array.from({ length: n }, (_, i) => entry(from + i));

  function add(s: Omit<MockSession, "id" | "name" | "importedAt" | "notes">, notes = null) {
    sessions.push({
      ...s,
      id: nextId++,
      name: s.sourceName,
      notes,
      importedAt: "2026-10-01T12:00:00Z",
    });
  }
  add({
    sourceName: "HISTORY 001",
    playedOn: "2026-02-14",
    venue: null,
    excluded: true,
    excludedReason: "pre_baseline",
    entries: range(40, 18),
  });
  add({
    sourceName: "HISTORY 002",
    playedOn: "2026-06-20",
    venue: "Panorama Bar",
    excluded: false,
    excludedReason: null,
    entries: [
      ...range(1, 6),
      divider("---- PEAK ----"),
      ...range(7, 5),
      unmatched("Untitled Edit", "Unknown"),
      ...range(12, 8),
    ],
  });
  add({
    sourceName: "HISTORY 003",
    playedOn: "2026-06-27",
    venue: null,
    excluded: true,
    excludedReason: "user",
    entries: range(100, 12),
  });
  add({
    sourceName: "HISTORY 004",
    playedOn: "2026-09-05",
    venue: "Garage Noord",
    excluded: false,
    excludedReason: null,
    entries: [...range(3, 10), unmatched("Dub 4", "White Label"), ...range(200, 14)],
  });
  add({
    sourceName: "HISTORY 005",
    playedOn: null,
    venue: null,
    excluded: false,
    excludedReason: null,
    entries: range(300, 9),
  });

  function recompute(ids: Iterable<number>) {
    for (const id of ids) {
      const b = baseline.get(id);
      if (b === undefined) continue;
      let count = b.count;
      let last = b.last;
      for (const s of sessions) {
        if (s.excluded) continue;
        const n = s.entries.filter((e) => e.kind === "track" && e.trackId === id).length;
        count += n;
        if (n > 0 && s.playedOn !== null && (last === null || s.playedOn > last)) last = s.playedOn;
      }
      const t = track(id);
      t.playCount = count;
      t.lastPlayed = last;
    }
  }
  const trackIds = (s: MockSession) =>
    s.entries.flatMap((e) => (e.trackId === null ? [] : [e.trackId]));
  recompute(sessions.flatMap(trackIds));

  function row(s: MockSession): SessionRow {
    const t = s.entries.filter((e) => e.kind === "track");
    return {
      id: s.id,
      name: s.name,
      sourceName: s.sourceName,
      playedOn: s.playedOn,
      venue: s.venue,
      excluded: s.excluded,
      excludedReason: s.excludedReason,
      tracks: t.length,
      unmatched: t.filter((e) => e.trackId === null).length,
      importedAt: s.importedAt,
    };
  }
  function sorted(): MockSession[] {
    return [...sessions].sort((a, b) => {
      if (a.playedOn === null || b.playedOn === null) {
        if (a.playedOn !== b.playedOn) return a.playedOn === null ? 1 : -1;
        return b.id - a.id;
      }
      return a.playedOn < b.playedOn ? 1 : a.playedOn > b.playedOn ? -1 : b.id - a.id;
    });
  }
  function find(id: number): MockSession {
    const s = sessions.find((x) => x.id === id);
    if (s === undefined) throw new Error(`play session ${id} not found`);
    return s;
  }
  function detail(id: number): SessionDetail {
    const s = find(id);
    return {
      session: row(s),
      notes: s.notes,
      volumeSerial: "1234-ABCD",
      entries: s.entries.map((e, position): SessionEntryRow => {
        const t = e.trackId === null ? null : track(e.trackId);
        return {
          position,
          kind: e.kind,
          trackId: e.trackId,
          title: t?.title ?? null,
          artist: t?.artist ?? null,
          durationMs: t?.durationMs ?? null,
          bpmX100: t?.bpmX100 ?? null,
          key: t?.key ?? null,
          rawTitle: e.rawTitle,
          rawArtist: e.rawArtist,
          rawPath: e.rawPath,
          rawDurationMs: e.rawDurationMs,
        };
      }),
    };
  }
  function update(id: number, patch: SessionPatch) {
    const s = find(id);
    if (patch.name !== undefined) {
      const name = patch.name.trim();
      if (name === "") throw new Error("invalid name: must not be empty");
      s.name = name;
    }
    if (patch.venue !== undefined) s.venue = patch.venue?.trim() || null;
    if (patch.notes !== undefined) s.notes = patch.notes?.trim() || null;
    if (patch.excluded !== undefined && patch.excluded !== s.excluded) {
      s.excluded = patch.excluded;
      s.excludedReason = patch.excluded ? "user" : null;
      recompute(trackIds(s));
    }
  }
  function trackSessions(id: number): SessionRef[] {
    return sorted().flatMap((s) => {
      const plays = s.entries.filter((e) => e.kind === "track" && e.trackId === id).length;
      return plays === 0
        ? []
        : [
            {
              id: s.id,
              name: s.name,
              playedOn: s.playedOn,
              venue: s.venue,
              excluded: s.excluded,
              plays,
            },
          ];
    });
  }

  /** The stick's sessions: one already imported, two new (one dated before the baseline). */
  function scan(source: ScanSource): HistoryScan {
    const planned = (
      sourceName: string,
      playedOn: string | null,
      entries: MockEntry[],
    ): PlannedSession => ({
      sourceName,
      playedOn,
      fingerprint: `${sourceName.replace(/\D/g, "")}f00dfeedbeef${playedOn ?? "undated"}`,
      action: { kind: "new" },
      entries: entries.map((e, position) => ({
        position,
        kind: e.kind,
        trackId: e.trackId,
        matchedBy: e.trackId === null ? (e.kind === "divider" ? "manifest" : null) : "manifest",
        raw: {
          stickPath: e.rawPath,
          title: e.rawTitle,
          artist: e.rawArtist,
          durationMs: e.rawDurationMs,
        },
      })),
    });
    const fresh = [
      planned("HISTORY 006", "2026-10-11", [
        ...range(500, 7),
        unmatched("ID", "ID"),
        ...range(507, 6),
      ]),
      planned("HISTORY 007", "2026-10-18", range(600, 15)),
      planned("HISTORY 008", "2026-02-02", range(700, 4)),
    ].filter((p) => !sessions.some((s) => s.sourceName === p.sourceName));
    const skipped: PlannedSession = {
      sourceName: "HISTORY 004",
      playedOn: "2026-09-05",
      fingerprint: "4c0ffee",
      action: { kind: "skip", reason: "alreadyImported", sessionId: 4 },
      entries: [],
    };
    lastPlan = {
      source: "device_library",
      volumeSerial: "1234-ABCD",
      baselineDate: BASELINE_DATE,
      sessions: [skipped, ...fresh],
    };
    planId += 1;
    return {
      planId,
      source:
        source.kind === "stick"
          ? { kind: "stick", path: (source.device as DeviceInfo).mountPoint }
          : { kind: "backup", path: source.path },
      plan: lastPlan,
    };
  }
  function importPlan(id: number, choices: ImportChoices): HistoryImportReport {
    if (lastPlan === null || id !== planId) {
      throw new Error("this dry run is out of date or was already imported; scan again");
    }
    const report: HistoryImportReport = {
      created: [],
      extended: [],
      skipped: 0,
      entries: 0,
      unmatched: 0,
    };
    for (const p of lastPlan.sessions) {
      if (p.action.kind !== "new") {
        report.skipped += 1;
        continue;
      }
      const excluded = choices.excluded[p.fingerprint] ?? false;
      const entries: MockEntry[] = p.entries.map((e) => ({
        kind: e.kind,
        trackId: e.trackId,
        rawTitle: e.raw.title,
        rawArtist: e.raw.artist,
        rawPath: e.raw.stickPath,
        rawDurationMs: e.raw.durationMs,
      }));
      add({
        sourceName: p.sourceName,
        playedOn: p.playedOn,
        venue: null,
        excluded,
        excludedReason: excluded ? "user" : null,
        entries,
      });
      const s = sessions[sessions.length - 1] as MockSession;
      report.created.push(s.id);
      report.entries += entries.length;
      report.unmatched += entries.filter((e) => e.kind === "track" && e.trackId === null).length;
      recompute(trackIds(s));
    }
    lastPlan = null;
    return report;
  }

  /** Handles a `history_*` command; `undefined` for any other. */
  return function handle(cmd: string, args: Record<string, unknown>): unknown {
    switch (cmd) {
      case "history_sessions": {
        const q = args.query as SessionQuery;
        return sorted()
          .filter((s) => q.excluded === undefined || s.excluded === q.excluded)
          .map(row);
      }
      case "history_session":
        return detail(args.id as number);
      case "history_session_update":
        update(args.id as number, args.patch as SessionPatch);
        return null;
      case "history_session_delete": {
        const s = find(args.id as number);
        sessions = sessions.filter((x) => x.id !== s.id);
        recompute(trackIds(s));
        return null;
      }
      case "history_link_entry": {
        const s = find(args.session as number);
        const e = s.entries[args.position as number];
        if (e === undefined || e.kind !== "track") throw new Error("not a track entry");
        const old = e.trackId;
        e.trackId = args.track as number;
        recompute(old === null ? [e.trackId] : [old, e.trackId]);
        return null;
      }
      case "history_track_sessions":
        return trackSessions(args.id as number);
      case "history_scan":
        return new Promise((resolve) =>
          setTimeout(() => resolve(scan(args.source as ScanSource)), 400),
        );
      case "history_import":
        return importPlan(args.planId as number, args.choices as ImportChoices);
      default:
        return undefined;
    }
  };
}
