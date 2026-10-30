/**
 * One deck, laid out like rekordbox's 2Deck Horizontal: header, zoomed
 * waveform, overview, then the controls row (CUE / PLAY, the pads, the quick
 * loop block, MASTER / SYNC), with the tempo column down the right edge. In
 * 2-deck mode the deck is as tall as the mixer and the waveform takes the
 * spare height; `compact` (4 decks) is the same deck 216 px tall. The
 * controls scale with the deck's width (`data-deck-size`, tokens.css).
 * The same in every state: what can't act yet is disabled, not hidden. Drop a track
 * from the table to load it, or drag one from another deck's header to move it here as
 * it plays. `solo` is the library's player deck on its own,
 * full window width: CUE / PLAY left of the waveforms and the pads right of
 * them, so the deck is one row shorter; no MASTER or SYNC (nothing to sync
 * to), and no mixer keys.
 *
 * Grid mode (`gridMode`, D10) puts its pads in the hot cue pads' cells and
 * its BPM in the loop block's, so nothing else moves; Q's cell is left
 * empty, MASTER and SYNC stay. A press on a section marker drags it; the
 * deck has the focus ring while the grid keys reach it.
 *
 * Layout and state selection only; each row is its own component.
 */
import { type DragEvent, type ReactNode, useState } from "react";

import { cn } from "../../lib/cn";
import { zoomSeconds } from "../../lib/deckScroll";
import { DECK_MIME, hasType, readDeck, readTracks, TRACKS_MIME } from "../../lib/dnd";
import type { CueEdit, DeckView } from "../../lib/ipc";
import { PLAYER_PLAY_KEY, WAVE_ZOOM_KEY } from "../../lib/keys";
import { type DeckAction, deckKeyHint } from "../../lib/mixKeys";
import { type LoopPage, loopBlockState, type PadMode, type TempoRange } from "../../lib/mixView";
import type { DeckMeta, PagedPadMode } from "../../store/mix";
import { DeckGridPanel } from "./DeckGridPanel";
import { DeckHeader } from "./DeckHeader";
import { DeckPads } from "./DeckPads";
import { DeckSlot, type DeckSlotState } from "./DeckSlot";
import { DeckSyncButtons } from "./DeckSyncButtons";
import { DeckTempo } from "./DeckTempo";
import { DeckTransport } from "./DeckTransport";
import { DeckWaveform } from "./DeckWaveform";
import { DeckWaveformRow } from "./DeckWaveformRow";
import { LoopBlock } from "./LoopBlock";
import { useDeckDetail } from "./useDeckDetail";
import type { DeckGridMode } from "./useDeckGridModes";
import { useDeckScrub } from "./useDeckScrub";
import { useMarkerDrag } from "./useMarkerDrag";

export interface DeckActions {
  togglePlay: () => void;
  play: () => void;
  pause: () => void;
  /** CUE pressed (`down`) or let go. */
  cue: (down: boolean) => void;
  toggleSync: () => void;
  takeMaster: () => void;
  toggleQuantise: () => void;
  toggleMasterTempo: () => void;
  /** Jumps to a set hot cue, sets an empty one at the playhead. */
  hotCuePad: (slot: number) => void;
  /** Sets, renames or deletes a cue (stored in the library). */
  editCues: (edit: CueEdit) => void;
  /** Cue point and playhead to `ms` (a memory cue); a memory loop's `endMs` arms RELOOP. */
  callCue: (ms: number, endMs: number | null) => void;
  /** Signed beats. */
  beatJump: (beats: number) => void;
  loopPad: (beats: number) => void;
  /** The quick-loop block (`LoopBlock`); the size button toggles the loop. */
  toggleLoop: () => void;
  loopIn: () => void;
  loopOut: () => void;
  reloop: () => void;
  loopResize: (double: boolean) => void;
  setPadMode: (mode: PadMode) => void;
  setPadPage: (mode: PagedPadMode, page: number) => void;
  setLoopPage: (page: LoopPage) => void;
  /** -1 slower, 1 faster, 0 released. */
  nudge: (dir: -1 | 0 | 1) => void;
  setTempo: (ratio: number) => void;
  setTempoRange: (range: TempoRange) => void;
  setBpm: (bpm: number) => void;
  /** Zooms every deck's scrolling waveform in (1) or out (-1). */
  zoom: (dir: -1 | 1) => void;
  /** `exact`: without quantise (grid mode). */
  seek: (ms: number, exact?: boolean) => void;
  eject: () => void;
  drop: (trackId: number) => void;
  /** Deck `from`'s track dropped here: it moves here as it plays. */
  dropDeck: (from: number) => void;
  /** Loads the track that failed again, or fetches the waveform again. */
  retry: () => void;
}

export interface DeckStripProps {
  deck: number;
  view: DeckView | undefined;
  meta: DeckMeta;
  /** Scrolling waveform zoom (`DECK_ZOOM_S` index), shared by all decks. */
  zoom: number;
  /** The track's key in the chosen notation. */
  keyText: string | null;
  /** Beats from the master's phase; null when there is none to compare. */
  phase: number | null;
  /** Shown alone as the player: hides the sync controls. */
  solo?: boolean;
  /** 4-deck mode: a shorter deck, the quick-loop grid beside the pads. */
  compact?: boolean;
  /** Shown in the header after the title (the player's "mixer not at default" chip). */
  notice?: ReactNode;
  /** The GRID toggle and, while it is on, the grid panel and marks. */
  gridMode?: DeckGridMode;
  actions: DeckActions;
}

/** What fills the waveform slot instead of the views, if anything. */
function slotState(meta: DeckMeta, loaded: boolean): DeckSlotState | null {
  if (meta.loading !== null) return { kind: "loading" };
  if (!loaded)
    return meta.error !== null
      ? { kind: "error", message: meta.error, playing: false }
      : { kind: "empty" };
  if (meta.analysing) return { kind: "analysing" };
  if (meta.columns === null && meta.viewError !== null)
    return { kind: "error", message: meta.viewError, playing: true };
  return null;
}

/** The line inside the waveform's bottom edge, if any. */
function warning(meta: DeckMeta, grid: boolean): string | null {
  if (meta.error !== null) return meta.error;
  if (meta.viewError !== null) return meta.viewError;
  if (meta.gridPending) return "Beatgrid ready: it takes effect when the deck is paused.";
  if (!grid) return "No beatgrid: sync, quantise, beat jump and loops need one. Analyse the track.";
  return null;
}

export function DeckStrip({
  deck,
  view,
  meta,
  zoom,
  keyText,
  phase,
  solo = false,
  compact = false,
  notice,
  gridMode,
  actions,
}: DeckStripProps) {
  const [dropping, setDropping] = useState(false);
  // This deck's own track is being dragged: it can't drop on itself.
  const [dragOut, setDragOut] = useState(false);
  const loaded = meta.track !== null && view?.loaded === true;
  const grid = loaded && view?.beat != null;
  const tempo = view?.tempo ?? 1;
  // The player deck alone has no mixer keys; Space plays deck 2 in both.
  const keyFor = (action: DeckAction) => deckKeyHint(action, deck);
  const playKey = [keyFor({ type: "play" }), deck === 1 ? PLAYER_PLAY_KEY.label : undefined]
    .filter((k) => k !== undefined)
    .join(" / ");
  // Grid mode places the playhead between beats, Q or not.
  const seek = gridMode?.on === true ? (ms: number) => actions.seek(ms, true) : actions.seek;
  const scrub = useDeckScrub({
    positionMs: view?.positionMs ?? 0,
    durationMs: view?.durationMs ?? 0,
    playing: view?.playing ?? false,
    viewS: zoomSeconds(zoom),
    seek,
    play: actions.play,
    pause: actions.pause,
  });
  const detail = useDeckDetail(
    deck,
    loaded ? meta.load : null,
    view?.durationMs ?? 0,
    view?.positionMs ?? 0,
  );
  const slot = slotState(meta, loaded);
  const gridPanel = gridMode?.on === true ? gridMode.panel : null;
  const drag = useMarkerDrag(
    gridMode?.overlay ?? null,
    view?.positionMs ?? 0,
    zoomSeconds(zoom),
    scrub,
  );

  const viewsBody =
    slot !== null || view === undefined ? (
      <DeckSlot
        deck={deck}
        state={slot ?? { kind: "empty" }}
        solo={solo}
        onRetry={actions.retry}
        onEject={actions.eject}
      />
    ) : (
      <div {...drag.handlers} className="flex min-h-0 flex-1 cursor-ew-resize touch-none flex-col">
        <DeckWaveform
          deck={deck}
          columns={meta.columns}
          detail={detail}
          beats={meta.beats}
          grid={drag.marks}
          positionMs={view.positionMs}
          durationMs={view.durationMs}
          playing={view.playing}
          speed={view.speed}
          cueMs={view.cueMs}
          hotCues={meta.hotCues}
          memoryCues={meta.memoryCues}
          loop={view.loopRegion}
          loopInMs={view.loopIn}
          zoom={zoom}
          onSeek={seek}
          onZoom={actions.zoom}
          overlay={
            <DeckWaveformRow
              deck={deck}
              zoom={zoom}
              zoomOutKey={WAVE_ZOOM_KEY.out}
              zoomInKey={WAVE_ZOOM_KEY.in}
              onZoom={actions.zoom}
              phaseShown={!solo && view.sync && !view.master}
              phase={phase}
              phaseHalf={view.syncBar ? 2 : 0.5}
              warning={warning(meta, grid)}
            />
          }
        />
      </div>
    );
  const transport = (
    <DeckTransport
      deck={deck}
      loaded={loaded}
      playing={view?.playing ?? false}
      cueKey={keyFor({ type: "cue" })}
      playKey={playKey === "" ? undefined : playKey}
      grid={grid}
      quantise={loaded && (view?.quantise ?? false)}
      showQuantise={gridPanel === null}
      onCue={actions.cue}
      onTogglePlay={actions.togglePlay}
      onQuantise={actions.toggleQuantise}
    />
  );
  const pads = (
    <DeckPads
      deck={deck}
      loaded={loaded}
      grid={grid}
      mode={meta.padMode}
      page={meta.padPage}
      hotCues={meta.hotCues}
      memoryCues={meta.memoryCues}
      loopBeats={view?.loopRegion?.beats ?? null}
      lastJump={meta.lastJump}
      lastLoop={meta.lastLoop}
      keyFor={keyFor}
      onMode={actions.setPadMode}
      onPage={actions.setPadPage}
      onHotCuePad={actions.hotCuePad}
      onEditCues={actions.editCues}
      onCallCue={actions.callCue}
      onJump={actions.beatJump}
      onLoop={actions.loopPad}
    />
  );

  const sync = (
    <DeckSyncButtons
      deck={deck}
      ready={grid}
      master={view?.master ?? false}
      sync={view?.sync ?? false}
      masterKey={keyFor({ type: "master" })}
      syncKey={keyFor({ type: "sync" })}
      onMaster={actions.takeMaster}
      onSync={actions.toggleSync}
    />
  );

  const loopBlock = (
    <LoopBlock
      deck={deck}
      state={loopBlockState(view, loaded, grid, meta.lastLoop)}
      loaded={loaded}
      page={meta.loopPage}
      onPage={actions.setLoopPage}
      keyFor={keyFor}
      actions={actions}
    />
  );

  const panel = gridPanel === null ? null : <DeckGridPanel {...gridPanel} />;

  const views = (
    <div
      className={cn("flex min-h-0 flex-col", solo ? "h-(--deck-views-h) min-w-0 flex-1" : "flex-1")}
    >
      {viewsBody}
    </div>
  );

  function onDragStart(e: DragEvent) {
    e.dataTransfer.setData(DECK_MIME, String(deck));
    e.dataTransfer.effectAllowed = "move";
    setDragOut(true);
  }

  function onDragOver(e: DragEvent) {
    const fromDeck = hasType(e.dataTransfer, DECK_MIME);
    if (fromDeck ? dragOut : !hasType(e.dataTransfer, TRACKS_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = fromDeck ? "move" : "copy";
    setDropping(true);
  }

  function onDrop(e: DragEvent) {
    setDropping(false);
    const from = readDeck(e.dataTransfer);
    if (from !== null) {
      if (from === deck) return;
      e.preventDefault();
      actions.dropDeck(from);
      return;
    }
    const id = readTracks(e.dataTransfer)?.trackIds[0];
    if (id === undefined) return;
    e.preventDefault();
    actions.drop(id);
  }

  return (
    <section
      aria-label={`Deck ${deck + 1}`}
      data-deck-size={solo ? "solo" : compact ? "compact" : "mix"}
      data-deck={deck}
      data-dropping={dropping || undefined}
      data-grid-focus={gridMode?.focused || undefined}
      onDragOver={onDragOver}
      onDragLeave={() => setDropping(false)}
      onDrop={onDrop}
      className={cn(
        "@container flex min-w-(--deck-min-w) shrink-0 gap-2 rounded-md border border-border bg-surface p-2 data-[dropping]:border-dashed data-[dropping]:border-accent data-[grid-focus]:border-accent",
        solo ? "h-(--deck-solo-h)" : compact ? "h-(--deck-h-compact)" : "h-(--deck-h)",
      )}
    >
      <div className={cn("flex min-w-0 flex-1 flex-col", compact ? "gap-1" : "gap-1.5")}>
        <DeckHeader
          deck={deck}
          loaded={loaded}
          track={meta.track}
          loading={meta.loading}
          keyText={keyText}
          bpm={grid ? (view?.bpm ?? null) : null}
          tempo={tempo}
          bpmLocked={
            !grid ? "Needs a beatgrid" : view?.sync ? "Synced: the BPM follows the master" : null
          }
          positionMs={loaded ? (view?.positionMs ?? 0) : 0}
          durationMs={loaded ? (view?.durationMs ?? 0) : 0}
          onBpm={actions.setBpm}
          onEject={actions.eject}
          notice={notice}
          drag={solo ? undefined : { onDragStart, onDragEnd: () => setDragOut(false) }}
          grid={
            gridMode === undefined
              ? undefined
              : { available: gridMode.available, on: gridMode.on, onToggle: gridMode.toggle }
          }
        />

        {solo ? (
          <div className="flex min-h-0 flex-1 gap-2">
            {transport}
            {views}
            {panel === null ? (
              <>
                {loopBlock}
                <div className="flex w-(--deck-pads-max-w) shrink-0">{pads}</div>
              </>
            ) : (
              <div className="flex w-(--deck-grid-w) shrink-0 flex-col">{panel}</div>
            )}
          </div>
        ) : (
          <>
            {views}
            <div className="grid h-(--deck-controls-h) shrink-0 grid-cols-(--deck-ctl-cols) gap-x-(--deck-gap)">
              {transport}
              {panel === null ? (
                <>
                  {pads}
                  {loopBlock}
                  {sync}
                </>
              ) : (
                <div className="col-span-3 grid min-w-0">{panel}</div>
              )}
            </div>
          </>
        )}
      </div>

      {/* Hidden in grid mode, its space kept so the waveform doesn't move. */}
      <div className={cn("flex", gridPanel !== null && "invisible")} inert={gridPanel !== null}>
        <DeckTempo
          deck={deck}
          loaded={loaded}
          tempo={tempo}
          range={meta.tempoRange}
          masterTempo={loaded && (view?.masterTempo ?? false)}
          // The 2-deck fader is about twice as long as the player deck's; the 4-deck one is about as long.
          ticks={solo || compact ? 7 : 13}
          slowerKey={keyFor({ type: "nudge", dir: -1 })}
          fasterKey={keyFor({ type: "nudge", dir: 1 })}
          onTempo={actions.setTempo}
          onRange={actions.setTempoRange}
          onMasterTempo={actions.toggleMasterTempo}
          onNudge={actions.nudge}
        />
      </div>
    </section>
  );
}
