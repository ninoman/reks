/**
 * One deck, laid out like rekordbox's 2Deck Horizontal: header, zoomed
 * waveform, overview, then CUE / PLAY, the pads and MASTER / SYNC, with the
 * tempo column down the right edge. One size in 2- and 4-deck mode, the same
 * in every state: what can't act yet is disabled, not hidden. Drop a track
 * from the table to load it. `solo` is the library's player deck on its own,
 * full window width: CUE / PLAY left of the waveforms and the pads right of
 * them, so the deck is one row shorter; no MASTER or SYNC (nothing to sync
 * to), and no mixer keys.
 *
 * Layout and state selection only; each row is its own component.
 */
import { type DragEvent, type ReactNode, useState } from "react";

import { cn } from "../../lib/cn";
import { zoomSeconds } from "../../lib/deckScroll";
import { hasType, readTracks, TRACKS_MIME } from "../../lib/dnd";
import type { CueEdit, DeckView } from "../../lib/ipc";
import { PLAYER_PLAY_KEY, WAVE_ZOOM_KEY } from "../../lib/keys";
import { type DeckAction, deckKeyHint } from "../../lib/mixKeys";
import { loopBlockState, type PadMode, type TempoRange } from "../../lib/mixView";
import type { DeckMeta, PagedPadMode } from "../../store/mix";
import { DeckHeader } from "./DeckHeader";
import { DeckPads } from "./DeckPads";
import { DeckSlot, type DeckSlotState } from "./DeckSlot";
import { DeckSyncButtons } from "./DeckSyncButtons";
import { DeckTempo } from "./DeckTempo";
import { DeckTransport } from "./DeckTransport";
import { DeckWaveform } from "./DeckWaveform";
import { DeckWaveformRow } from "./DeckWaveformRow";
import { LoopBlock } from "./LoopBlock";
import { useDeckScrub } from "./useDeckScrub";

export interface DeckActions {
  togglePlay: () => void;
  play: () => void;
  pause: () => void;
  /** CUE pressed (`down`) or let go. */
  cue: (down: boolean) => void;
  toggleSync: () => void;
  takeMaster: () => void;
  toggleQuantise: () => void;
  /** Jumps to a set hot cue, sets an empty one at the playhead. */
  hotCuePad: (slot: number) => void;
  /** Sets, renames or deletes a cue (stored in the library). */
  editCues: (edit: CueEdit) => void;
  /** Cue point and playhead to `ms` (a memory cue); a memory loop's `endMs` arms RELOOP. */
  callCue: (ms: number, endMs: number | null) => void;
  /** Signed beats. */
  beatJump: (beats: number) => void;
  loopPad: (beats: number) => void;
  /** The quick-loop block (`LoopBlock`). */
  toggleLoop: () => void;
  loopIn: () => void;
  loopOut: () => void;
  reloop: () => void;
  loopResize: (double: boolean) => void;
  setPadMode: (mode: PadMode) => void;
  setPadPage: (mode: PagedPadMode, page: number) => void;
  /** -1 slower, 1 faster, 0 released. */
  nudge: (dir: -1 | 0 | 1) => void;
  setTempo: (ratio: number) => void;
  setTempoRange: (range: TempoRange) => void;
  setBpm: (bpm: number) => void;
  /** Zooms every deck's scrolling waveform in (1) or out (-1). */
  zoom: (dir: -1 | 1) => void;
  seek: (ms: number) => void;
  eject: () => void;
  drop: (trackId: number) => void;
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
  /** Shown in the header after the title (the player's "mixer not at default" chip). */
  notice?: ReactNode;
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
  notice,
  actions,
}: DeckStripProps) {
  const [dropping, setDropping] = useState(false);
  const loaded = meta.track !== null && view?.loaded === true;
  const grid = loaded && view?.beat != null;
  const tempo = view?.tempo ?? 1;
  // The player deck alone has no mixer keys; Space plays deck 2 in both.
  const keyFor = (action: DeckAction) => deckKeyHint(action, deck);
  const playKey = [keyFor({ type: "play" }), deck === 1 ? PLAYER_PLAY_KEY.label : undefined]
    .filter((k) => k !== undefined)
    .join(" / ");
  const scrub = useDeckScrub({
    positionMs: view?.positionMs ?? 0,
    durationMs: view?.durationMs ?? 0,
    playing: view?.playing ?? false,
    viewS: zoomSeconds(zoom),
    seek: actions.seek,
    play: actions.play,
    pause: actions.pause,
  });
  const slot = slotState(meta, loaded);

  const views =
    slot !== null || view === undefined ? (
      <DeckSlot
        deck={deck}
        state={slot ?? { kind: "empty" }}
        solo={solo}
        onRetry={actions.retry}
        onEject={actions.eject}
      />
    ) : (
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
          loopInMs={view.loopIn}
          zoom={zoom}
          onSeek={actions.seek}
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

  // The mixer decks are too narrow for a block beside the pads (task 23 moves them): a row under the views.
  const loopBlock = (
    <LoopBlock
      deck={deck}
      layout={solo ? "grid" : "row"}
      state={loopBlockState(view, loaded, grid, meta.lastLoop)}
      loaded={loaded}
      keyFor={keyFor}
      actions={actions}
    />
  );

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
      aria-label={`Deck ${deck + 1}`}
      data-dropping={dropping || undefined}
      onDragOver={onDragOver}
      onDragLeave={() => setDropping(false)}
      onDrop={onDrop}
      className={cn(
        "flex min-w-(--deck-min-w) shrink-0 gap-2 rounded-md border border-border bg-surface p-2 data-[dropping]:border-dashed data-[dropping]:border-accent",
        solo ? "h-(--deck-solo-h)" : "h-(--deck-h)",
      )}
    >
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
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
        />

        {solo ? (
          <div className="flex min-h-0 flex-1 gap-2">
            {transport}
            <div className="min-w-0 flex-1">{views}</div>
            {loopBlock}
            <div className="flex w-(--deck-pads-max-w) shrink-0">{pads}</div>
          </div>
        ) : (
          <>
            {views}
            {loopBlock}
            <div className="flex h-(--deck-bottom-h) shrink-0 gap-2">
              {transport}
              {pads}
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
            </div>
          </>
        )}
      </div>

      <DeckTempo
        deck={deck}
        loaded={loaded}
        tempo={tempo}
        range={meta.tempoRange}
        // The mixer decks' fader is about twice as long as the player deck's.
        ticks={solo ? 7 : 13}
        slowerKey={keyFor({ type: "nudge", dir: -1 })}
        fasterKey={keyFor({ type: "nudge", dir: 1 })}
        onTempo={actions.setTempo}
        onRange={actions.setTempoRange}
        onNudge={actions.nudge}
      />
    </section>
  );
}
