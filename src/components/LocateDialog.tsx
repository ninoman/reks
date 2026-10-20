/**
 * "Locate…" for an unmatched session entry: search the library (starting
 * from the stick's artist and title) and pick the track it was. Linking
 * counts the play for that track (unless the session is practice).
 */
import { useEffect, useState } from "react";

import { formatDuration } from "../lib/format";
import { libraryQuery, type SessionEntryRow, type TrackRow } from "../lib/ipc";
import { useHistory } from "../store/history";
import { defaultSort } from "../store/library";
import { Button } from "./ui/Button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { Input } from "./ui/Input";
import { ScrollArea } from "./ui/ScrollArea";

const LIMIT = 50;

export function LocateDialog({ entry }: { entry: SessionEntryRow }) {
  const h = useHistory.getState;
  const [text, setText] = useState(
    [entry.rawArtist, entry.rawTitle].filter((v) => v !== null && v !== "").join(" "),
  );
  const [rows, setRows] = useState<TrackRow[] | null>(null);
  const [picked, setPicked] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      libraryQuery({
        text: text.trim() === "" ? null : text,
        filters: [],
        sort: defaultSort,
        offset: 0,
        limit: LIMIT,
      }).then(
        (page) => {
          if (!live) return;
          setRows(page.rows);
          setError(null);
        },
        (e: unknown) => live && setError(String(e)),
      );
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [text]);

  return (
    <Dialog open onOpenChange={(open) => !open && h().startLocate(null)}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Locate track</DialogTitle>
          <DialogDescription>
            The stick played “{entry.rawTitle ?? "untitled"}”
            {entry.rawArtist ? ` by ${entry.rawArtist}` : ""}
            {entry.rawDurationMs !== null && ` (${formatDuration(entry.rawDurationMs)})`}. Pick the
            library track it was.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-2">
          <div className="truncate font-mono text-xs text-fg-muted select-text">
            {entry.rawPath ?? "No path on the stick"}
          </div>
          <Input
            variant="search"
            aria-label="Search library"
            placeholder="Search title, artist…"
            value={text}
            autoFocus
            onChange={(e) => setText(e.target.value)}
          />
          <ScrollArea className="h-72 rounded-md border border-border">
            <div role="listbox" aria-label="Library tracks">
              {rows?.map((r) => (
                <div
                  key={r.id}
                  role="option"
                  aria-selected={picked === r.id}
                  tabIndex={-1}
                  className="flex h-(--row-h) cursor-default items-center gap-2 px-2 text-md odd:bg-row-alt aria-selected:bg-selected aria-selected:text-selected-fg"
                  onClick={() => setPicked(r.id)}
                  onKeyDown={(e) => e.key === "Enter" && setPicked(r.id)}
                  onDoubleClick={() => void h().link(entry.position, r.id)}
                >
                  <span className="flex-1 truncate">{r.title}</span>
                  <span className="w-1/3 truncate text-fg-muted">{r.artist}</span>
                  <span className="w-12 text-right tabular-nums text-fg-muted">
                    {formatDuration(r.durationMs)}
                  </span>
                </div>
              ))}
              {rows?.length === 0 && <p className="m-2 text-fg-muted">No matching tracks.</p>}
            </div>
          </ScrollArea>
          {error !== null && (
            <div className="text-danger select-text" role="alert">
              {error}
            </div>
          )}
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => h().startLocate(null)}>Cancel</Button>
          <Button
            variant="primary"
            disabled={picked === null}
            onClick={() => picked !== null && void h().link(entry.position, picked)}
          >
            Link
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
