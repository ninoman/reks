/**
 * One deck: track, tempo and time; scrolling waveform and overview (drag
 * either to skim); CUE (held on the cue point: preview), PLAY, SYNC (BAR),
 * LEADER and QUANTISE; hot cues A–H; nudge; the tempo fader with its range.
 * Drop a track from the table to load it. Shows the empty, loading and
 * error states of the deck. `solo` is the library's player deck on its own:
 * no SYNC, BAR or LEADER (nothing to sync to).
 */
import { type DragEvent, useState } from "react";

import { cn } from "../../lib/cn";
import { zoomSeconds } from "../../lib/deckScroll";
import { hasType, readTracks, TRACKS_MIME } from "../../lib/dnd";
import { hotCueLabel } from "../../lib/format";
import type { DeckView } from "../../lib/ipc";
import {
  formatClock,
  formatTempo,
  nextTempoRange,
  type TempoRange,
  tempoFader,
  tempoRangeLabel,
  tempoRatio,
} from "../../lib/mixView";
import type { DeckMeta } from "../../store/mix";
import { EjectIcon, PauseIcon, PlayIcon } from "../icons";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Caption } from "../ui/Caption";
import { Fader } from "../ui/Fader";
import { IconButton } from "../ui/IconButton";
import { Tooltip } from "../ui/Tooltip";
import { DeckWaveform } from "./DeckWaveform";
import { SyncPhase } from "./SyncPhase";
import { useDeckScrub } from "./useDeckScrub";

const HOT_CUES = 8;

export interface DeckActions {
  togglePlay: () => void;
  play: () => void;
  pause: () => void;
  /** CUE pressed (`down`) or let go. */
  cue: (down: boolean) => void;
  toggleSync: () => void;
  toggleBar: () => void;
  toggleLeader: () => void;
  toggleQuantise: () => void;
  hotCue: (slot: number) => void;
  /** -1 slower, 1 faster, 0 released. */
  nudge: (dir: -1 | 0 | 1) => void;
  setTempo: (ratio: number) => void;
  setTempoRange: (range: TempoRange) => void;
  /** Zooms the scrolling waveform in (1) or out (-1). */
  zoom: (dir: -1 | 1) => void;
  seek: (ms: number) => void;
  eject: () => void;
  drop: (trackId: number) => void;
}

export interface DeckStripProps {
  deck: number;
  view: DeckView | undefined;
  meta: DeckMeta;
  leader: boolean;
  /** Chosen as leader (not automatic). */
  leaderChosen: boolean;
  /** Beats from the leader's phase; null when there is none to compare. */
  phase: number | null;
  compact: boolean;
  /** Shown alone as the player: hides the sync controls. */
  solo?: boolean;
  actions: DeckActions;
}

function rangeName(r: TempoRange): string {
  return r === 1 ? "wide" : `${tempoRangeLabel(r)}%`;
}

export function DeckStrip({
  deck,
  view,
  meta,
  leader,
  leaderChosen,
  phase,
  compact,
  solo = false,
  actions,
}: DeckStripProps) {
  const [dropping, setDropping] = useState(false);
  const n = deck + 1;
  const loaded = meta.track !== null && view?.loaded === true;
  const tempo = view?.tempo ?? 1;
  const scrub = useDeckScrub({
    positionMs: view?.positionMs ?? 0,
    durationMs: view?.durationMs ?? 0,
    playing: view?.playing ?? false,
    viewS: zoomSeconds(meta.zoom),
    seek: actions.seek,
    play: actions.play,
    pause: actions.pause,
  });

  function onDragOver(e: DragEvent) {
    if (!hasType(e.dataTransfer, TRACKS_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    setDropping(true);
  }

  function onDrop(e: DragEvent) {
    setDropping(false);
    const id = readTracks(e.dataTransfer)?.trackIds[0];
    if (id === undefined) return;
    e.preventDefault();
    actions.drop(id);
  }

  return (
    <section
      aria-label={`Deck ${n}`}
      data-dropping={dropping || undefined}
      onDragOver={onDragOver}
      onDragLeave={() => setDropping(false)}
      onDrop={onDrop}
      className="flex min-w-0 gap-2 rounded-md border border-border bg-surface p-2 data-[dropping]:border-accent"
    >
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <header className="flex min-w-0 items-center gap-2">
          <Badge variant={loaded ? "accent" : "muted"} size="md">
            {n}
          </Badge>
          <div className="min-w-0 flex-1 truncate">
            {meta.track !== null ? (
              <>
                <span className="font-medium">{meta.track.title ?? "Untitled"}</span>
                {meta.track.artist !== null && (
                  <span className="text-fg-muted"> — {meta.track.artist}</span>
                )}
              </>
            ) : meta.loading !== null ? (
              <span className="text-fg-muted">Loading {meta.loading.title ?? "track"}…</span>
            ) : (
              <span className="text-fg-subtle">Empty</span>
            )}
          </div>
          <span className="font-mono text-xs tabular-nums" title="BPM">
            {view?.bpm != null ? view.bpm.toFixed(2) : "—"}
          </span>
          <span className="w-14 text-right font-mono text-xs text-fg-muted tabular-nums">
            {formatTempo(tempo)}
          </span>
          <Tooltip content="Eject">
            <IconButton
              size="xs"
              aria-label={`Eject deck ${n}`}
              disabled={!loaded}
              onClick={actions.eject}
            >
              <EjectIcon />
            </IconButton>
          </Tooltip>
        </header>

        {meta.error !== null && (
          <p role="alert" className="text-xs text-danger select-text">
            {meta.error}
          </p>
        )}

        {loaded && view !== undefined ? (
          <>
            <div {...scrub} className="cursor-ew-resize touch-none">
              <DeckWaveform
                deck={deck}
                columns={meta.columns}
                beats={meta.beats}
                positionMs={view.positionMs}
                durationMs={view.durationMs}
                playing={view.playing}
                speed={view.speed}
                cueMs={view.cueMs}
                hotCues={meta.hotCues}
                memoryCues={meta.memoryCues}
                loop={view.loopRegion}
                zoom={meta.zoom}
                compact={compact}
                onSeek={actions.seek}
                onZoom={actions.zoom}
              />
            </div>
            {(meta.viewError !== null || (meta.beats !== null && meta.beats.length === 0)) && (
              <p className="text-2xs text-warning">
                {meta.viewError ??
                  "No beatgrid: sync and quantise need one. Analyse the track first."}
              </p>
            )}
          </>
        ) : (
          <div
            className={cn(
              "flex items-center justify-center rounded-sm border border-dashed border-border-strong text-xs text-fg-muted",
              compact ? "h-(--deck-wave-h-compact)" : "h-(--deck-wave-h)",
            )}
          >
            {meta.loading !== null
              ? "Decoding…"
              : solo
                ? "Double-click a track to play it, or drag one here"
                : `Drag a track here, or right-click one and choose Load to deck ${n}`}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-1">
          <Button
            size="sm"
            disabled={!loaded}
            aria-label={`Deck ${n} cue`}
            onPointerDown={(e) => e.button === 0 && actions.cue(true)}
            onPointerUp={() => actions.cue(false)}
            onPointerCancel={() => actions.cue(false)}
            onPointerLeave={(e) => e.buttons !== 0 && actions.cue(false)}
            onKeyDown={(e) => {
              if ((e.key === "Enter" || e.key === " ") && !e.repeat) {
                e.preventDefault();
                actions.cue(true);
              }
            }}
            onKeyUp={(e) => {
              if (e.key === "Enter" || e.key === " ") actions.cue(false);
            }}
          >
            Cue
          </Button>
          <IconButton
            size="sm"
            variant="secondary"
            disabled={!loaded}
            aria-label={view?.playing ? `Pause deck ${n}` : `Play deck ${n}`}
            onClick={actions.togglePlay}
          >
            {view?.playing ? <PauseIcon /> : <PlayIcon />}
          </IconButton>
          {!solo && (
            <>
              <Button
                size="sm"
                disabled={!loaded}
                aria-pressed={view?.sync ?? false}
                aria-label={`Deck ${n} sync`}
                onClick={actions.toggleSync}
              >
                Sync
              </Button>
              <Button
                size="sm"
                disabled={!loaded}
                aria-pressed={view?.syncBar ?? false}
                aria-label={`Deck ${n} bar sync`}
                onClick={actions.toggleBar}
              >
                Bar
              </Button>
              <Button
                size="sm"
                disabled={!loaded}
                aria-pressed={leaderChosen && leader}
                aria-label={`Deck ${n} leader`}
                onClick={actions.toggleLeader}
              >
                Lead
              </Button>
            </>
          )}
          <Button
            size="sm"
            disabled={!loaded}
            aria-pressed={view?.quantise ?? false}
            aria-label={`Deck ${n} quantise`}
            onClick={actions.toggleQuantise}
          >
            Q
          </Button>
          {!solo && loaded && view?.sync === true && (
            <SyncPhase offset={phase} half={view.syncBar ? 2 : 0.5} leader={leader} />
          )}
          {!solo && loaded && view?.sync !== true && leader && (
            <SyncPhase offset={null} half={0.5} leader />
          )}
          <span className="ml-auto font-mono text-xs tabular-nums">
            {formatClock(view?.positionMs ?? 0)}
            <span className="ml-2 text-fg-muted">
              {formatClock((view?.durationMs ?? 0) - (view?.positionMs ?? 0), true)}
            </span>
          </span>
        </div>

        <div className="flex flex-wrap items-center gap-1">
          <Caption className="mr-1" aria-hidden="true">
            Hot cue
          </Caption>
          {Array.from({ length: compact ? 4 : HOT_CUES }, (_, slot) => {
            const set = meta.hotCues.some((c) => c.slot === slot);
            return (
              <Button
                // biome-ignore lint/suspicious/noArrayIndexKey: hot cue slots are fixed
                key={slot}
                size="xs"
                variant={set ? "secondary" : "ghost"}
                disabled={!loaded || !set}
                aria-label={`Deck ${n} hot cue ${hotCueLabel(slot)}`}
                onClick={() => actions.hotCue(slot)}
                className="w-(--ctl-h-sm) px-0"
              >
                {hotCueLabel(slot)}
              </Button>
            );
          })}
          <span className="ml-auto flex gap-1">
            {([-1, 1] as const).map((dir) => (
              <Button
                key={dir}
                size="xs"
                disabled={!loaded}
                aria-label={`Deck ${n} nudge ${dir < 0 ? "slower" : "faster"}`}
                onPointerDown={() => actions.nudge(dir)}
                onPointerUp={() => actions.nudge(0)}
                onPointerLeave={(e) => e.buttons !== 0 && actions.nudge(0)}
                onKeyDown={(e) => {
                  if ((e.key === "Enter" || e.key === " ") && !e.repeat) actions.nudge(dir);
                }}
                onKeyUp={(e) => {
                  if (e.key === "Enter" || e.key === " ") actions.nudge(0);
                }}
              >
                {dir < 0 ? "−" : "+"}
              </Button>
            ))}
          </span>
        </div>
      </div>

      <div className="flex flex-col items-center gap-1.5">
        <Tooltip content="Tempo range">
          <Button
            size="xs"
            aria-label={`Deck ${n} tempo range ${rangeName(meta.tempoRange)}`}
            onClick={() => actions.setTempoRange(nextTempoRange(meta.tempoRange))}
            className="w-(--fader-w) min-w-max"
          >
            {tempoRangeLabel(meta.tempoRange)}
          </Button>
        </Tooltip>
        <Fader
          disabled={!loaded}
          value={tempoFader(tempo, meta.tempoRange)}
          defaultValue={0.5}
          onValueChange={(v) => actions.setTempo(tempoRatio(v, meta.tempoRange))}
          aria-label={`Deck ${n} tempo`}
          valueText={formatTempo(tempo)}
        />
        <Button size="xs" variant="ghost" disabled={!loaded} onClick={() => actions.setTempo(1)}>
          Reset
        </Button>
      </div>
    </section>
  );
}
