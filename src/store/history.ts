/**
 * Play history (Zustand): the imported sessions for the Histories tree, the
 * session open in the main area, and the edits on it. Counts are derived in
 * reks-core (D23); after any change that can move them, the session list,
 * the open session and the track table are reloaded and `version` is bumped
 * so "Played in" refetches.
 */
import { create } from "zustand";

import {
  historyLinkEntry,
  historySession,
  historySessionDelete,
  historySessions,
  historySessionUpdate,
  type SessionDetail,
  type SessionPatch,
  type SessionRow,
} from "../lib/ipc";
import { refreshAll } from "./panes";

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export interface MonthGroup {
  /** `YYYY-MM`. */
  key: string;
  label: string;
  sessions: SessionRow[];
}

export interface YearGroup {
  /** `YYYY`, or `undated` for sessions with no date (HIST1). */
  key: string;
  label: string;
  /** Empty for `undated`. */
  months: MonthGroup[];
  /** The undated group's sessions; empty otherwise. */
  sessions: SessionRow[];
}

/** Year → month → session, in the list's order (newest first, undated last). */
export function groupSessions(rows: readonly SessionRow[]): YearGroup[] {
  const years: YearGroup[] = [];
  const undated: SessionRow[] = [];
  for (const s of rows) {
    if (s.playedOn === null) {
      undated.push(s);
      continue;
    }
    const y = s.playedOn.slice(0, 4);
    const m = s.playedOn.slice(0, 7);
    let year = years.find((g) => g.key === y);
    if (year === undefined) {
      year = { key: y, label: y, months: [], sessions: [] };
      years.push(year);
    }
    let month = year.months.find((g) => g.key === m);
    if (month === undefined) {
      month = { key: m, label: MONTHS[Number(m.slice(5)) - 1] ?? m, sessions: [] };
      year.months.push(month);
    }
    month.sessions.push(s);
  }
  if (undated.length > 0) {
    years.push({ key: "undated", label: "Date unknown", months: [], sessions: undated });
  }
  return years;
}

export interface HistoryState {
  sessions: SessionRow[];
  /** The Histories root row is open. */
  treeOpen: boolean;
  /** Open year and month groups (`YYYY`, `YYYY-MM`, `undated`). */
  expanded: ReadonlySet<string>;
  /** The session shown in the main area instead of the track table. */
  openId: number | null;
  detail: SessionDetail | null;
  /** Divider entries are hidden unless this is on. */
  showDividers: boolean;
  /** Session waiting for the delete confirmation. */
  confirmDelete: SessionRow | null;
  /** The entry "Locate…" is linking. */
  locating: number | null;
  importOpen: boolean;
  /** Bumped after every change that can move play counts. */
  version: number;
  error: string | null;

  load(): Promise<void>;
  toggleTree(): void;
  toggleGroup(key: string): void;
  open(id: number): Promise<void>;
  close(): void;
  setShowDividers(on: boolean): void;
  update(patch: SessionPatch): Promise<void>;
  requestDelete(): void;
  resolveDelete(confirmed: boolean): Promise<void>;
  startLocate(position: number | null): void;
  link(position: number, trackId: number): Promise<void>;
  setImportOpen(open: boolean): void;
  /** After an import: reload the list and the counts. */
  imported(): Promise<void>;
}

export const useHistory = create<HistoryState>()((set, get) => {
  async function reload(countsMoved: boolean): Promise<void> {
    await get().load();
    const id = get().openId;
    if (id !== null) {
      try {
        set({ detail: await historySession(id) });
      } catch (e) {
        set({ openId: null, detail: null, error: String(e) });
      }
    }
    if (countsMoved) {
      set((s) => ({ version: s.version + 1 }));
      refreshAll(true);
    }
  }

  async function run(f: () => Promise<void>, countsMoved: boolean): Promise<boolean> {
    try {
      await f();
      set({ error: null });
      await reload(countsMoved);
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  }

  return {
    sessions: [],
    treeOpen: true,
    expanded: new Set(),
    openId: null,
    detail: null,
    showDividers: false,
    confirmDelete: null,
    locating: null,
    importOpen: false,
    version: 0,
    error: null,

    async load() {
      try {
        const sessions = await historySessions();
        set((s) => {
          // Open the newest year and month the first time there are sessions.
          let expanded = s.expanded;
          if (s.sessions.length === 0 && sessions.length > 0) {
            const first = groupSessions(sessions)[0];
            expanded = new Set(
              first === undefined ? [] : [first.key, ...first.months.slice(0, 1).map((m) => m.key)],
            );
          }
          return { sessions, expanded };
        });
      } catch (e) {
        set({ error: String(e) });
      }
    },
    toggleTree() {
      set((s) => ({ treeOpen: !s.treeOpen }));
    },
    toggleGroup(key) {
      set((s) => {
        const expanded = new Set(s.expanded);
        if (!expanded.delete(key)) expanded.add(key);
        return { expanded };
      });
    },
    async open(id) {
      set({ openId: id, locating: null, error: null });
      try {
        const detail = await historySession(id);
        if (get().openId === id) set({ detail });
      } catch (e) {
        set({ error: String(e) });
      }
    },
    close() {
      if (get().openId !== null) set({ openId: null, detail: null, locating: null });
    },
    setShowDividers(showDividers) {
      set({ showDividers });
    },
    async update(patch) {
      const id = get().openId;
      if (id === null) return;
      await run(() => historySessionUpdate(id, patch), patch.excluded !== undefined);
    },
    requestDelete() {
      const id = get().openId;
      set((s) => ({ confirmDelete: s.sessions.find((x) => x.id === id) ?? null }));
    },
    async resolveDelete(confirmed) {
      const target = get().confirmDelete;
      set({ confirmDelete: null });
      if (!confirmed || target === null) return;
      if (get().openId === target.id) set({ openId: null, detail: null });
      await run(() => historySessionDelete(target.id), true);
    },
    startLocate(locating) {
      set({ locating });
    },
    async link(position, trackId) {
      const id = get().openId;
      if (id === null) return;
      if (await run(() => historyLinkEntry(id, position, trackId), true)) set({ locating: null });
    },
    setImportOpen(importOpen) {
      set({ importOpen });
    },
    async imported() {
      await reload(true);
    },
  };
});
