/**
 * The Histories tree in the sidebar, under the playlists: imported play
 * sessions by year → month → session (newest first; undated sessions under
 * "Date unknown"). Click a session (or Up/Down with the tree focused) to show
 * it in the main area. "+" imports history from a stick or a backup.
 * Practice sessions are dimmed.
 */
import type { KeyboardEvent } from "react";

import { groupSessions, useHistory } from "../store/history";
import { Disclosure } from "./Disclosure";
import { PlusIcon } from "./icons";
import { INDENT, NAME, PAD, ROW } from "./treeStyles";
import { focusRingInset } from "./ui/control";
import { IconButton } from "./ui/IconButton";
import { ScrollArea } from "./ui/ScrollArea";

const SESSION = `${ROW} data-[practice]:text-fg-muted`;

export function HistoryTree() {
  const sessions = useHistory((s) => s.sessions);
  const treeOpen = useHistory((s) => s.treeOpen);
  const expanded = useHistory((s) => s.expanded);
  const openId = useHistory((s) => s.openId);
  const error = useHistory((s) => s.error);
  const h = useHistory.getState;
  const groups = groupSessions(sessions);

  /** Up/Down open the previous/next session among the visible ones. */
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const visible = treeOpen
      ? groups.flatMap((y) =>
          expanded.has(y.key)
            ? [...y.months.flatMap((m) => (expanded.has(m.key) ? m.sessions : [])), ...y.sessions]
            : [],
        )
      : [];
    const i = visible.findIndex((s) => s.id === openId);
    const next = visible[i === -1 ? 0 : i + (e.key === "ArrowDown" ? 1 : -1)];
    if (next !== undefined) void h().open(next.id);
  }

  const group = (key: string, label: string, level: number) => (
    <div
      key={key}
      role="treeitem"
      aria-level={level}
      aria-selected={false}
      aria-expanded={expanded.has(key)}
      className={ROW}
      style={{ paddingLeft: PAD + (level - 1) * INDENT }}
      onClick={() => h().toggleGroup(key)}
    >
      <Disclosure open={expanded.has(key)} />
      <span className={NAME}>{label}</span>
    </div>
  );

  const session = (id: number, name: string, excluded: boolean, title: string, level: number) => (
    <div
      key={id}
      role="treeitem"
      aria-level={level}
      aria-selected={openId === id}
      className={SESSION}
      data-selected={openId === id || undefined}
      data-practice={excluded || undefined}
      style={{ paddingLeft: PAD + (level - 1) * INDENT }}
      title={title}
      onClick={() => void h().open(id)}
    >
      <span className="inline-flex w-3.5 flex-none" />
      <span className={NAME}>{name}</span>
      {excluded && <span className="text-xs text-fg-subtle">practice</span>}
    </div>
  );

  return (
    <section
      className="flex max-h-1/2 min-h-0 shrink-0 flex-col border-t border-border"
      aria-label="Histories"
    >
      <ScrollArea className="min-h-0">
        <div
          className={`flex flex-col py-1 ${focusRingInset}`}
          role="tree"
          aria-label="Histories"
          tabIndex={0}
          onKeyDown={onKeyDown}
        >
          <div
            role="treeitem"
            aria-level={1}
            aria-selected={false}
            aria-expanded={treeOpen}
            className={ROW}
            data-root=""
            onClick={() => h().toggleTree()}
          >
            <Disclosure open={treeOpen} />
            <span className={NAME}>Histories</span>
            <IconButton
              size="xs"
              shape="round"
              variant="outline"
              title="Import history…"
              aria-label="Import history"
              onClick={(e) => {
                e.stopPropagation();
                h().setImportOpen(true);
              }}
            >
              <PlusIcon />
            </IconButton>
          </div>
          {treeOpen && sessions.length === 0 && (
            <p
              className="m-0 py-1 pr-2 text-xs text-fg-subtle"
              style={{ paddingLeft: PAD + INDENT }}
            >
              No sessions yet. Import the history from a stick that was played on.
            </p>
          )}
          {treeOpen &&
            groups.map((y) => [
              group(y.key, y.label, 2),
              ...(expanded.has(y.key)
                ? [
                    ...y.months.flatMap((m) => [
                      group(m.key, m.label, 3),
                      ...(expanded.has(m.key)
                        ? m.sessions.map((s) =>
                            session(s.id, s.name, s.excluded, tip(s.playedOn, s.tracks), 4),
                          )
                        : []),
                    ]),
                    ...y.sessions.map((s) =>
                      session(s.id, s.name, s.excluded, tip(s.playedOn, s.tracks), 3),
                    ),
                  ]
                : []),
            ])}
        </div>
      </ScrollArea>
      {error !== null && (
        <div className="mx-2 my-1.5 text-danger select-text" role="alert">
          {error}
        </div>
      )}
    </section>
  );
}

function tip(playedOn: string | null, tracks: number): string {
  return `${playedOn ?? "Date unknown"} · ${tracks} track${tracks === 1 ? "" : "s"}`;
}
