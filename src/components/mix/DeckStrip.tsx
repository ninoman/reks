/**
 * One deck, laid out like rekordbox's 2Deck Horizontal: header, zoomed
 * waveform, overview, then CUE / PLAY, the pads and MASTER / SYNC, with the
 * tempo column down the right edge. One size in 2- and 4-deck mode, the same
 * in every state: what can't act yet is disabled, not hidden. Drop a track
 * from the table to load it. `solo` is the library's player deck on its own:
 * no MASTER or SYNC (nothing to sync to), and no mixer keys.
 *
 * Layout and state selection only; each row is its own component.
 */
import { type DragEvent, useState } from "react";

import { zoomSeconds } from "../../lib/deckScroll";
import { hasType, readTracks, TRACKS_MIME } from "../../lib/dnd";
import type { DeckView } from "../../lib/ipc";
import { PLAYER_PLAY_KEY, WAVE_ZOOM_KEY } from "../../lib/keys";
import { type DeckAction, deckKeyHint } from "../../lib/mixKeys";
import type { PadMode, TempoRange } from "../../lib/mixView";
import type { DeckMeta, PagedPadMode } from "../../store/mix";
import { DeckHeader } from "./DeckHeader";
import { DeckPads } from "./DeckPads";
import { DeckSlot, type DeckSlotState } from "./DeckSlot";
import { DeckSyncButtons } from "./DeckSyncButtons";
import { DeckTempo } from "./DeckTempo";
import { DeckTransport } from "./DeckTransport";
import { DeckWaveform } from "./DeckWaveform";
import { DeckWaveformRow } from "./DeckWaveformRow";
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
  hotCue: (slot: number) => void;
  /** Signed beats. */
  beatJump: (beats: number) => void;
  loopPad: (beats: number) => void;
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
  actions,
}: DeckStripProps) {
  const [dropping, setDropping] = useState(false);
  const loaded = meta.track !== null && view?.loaded === true;
  const grid = loaded && view?.beat != null;
  const tempo = view?.tempo ?? 1;
  // The player deck alone has no mixer keys; Space plays deck 2 in both.
  const keyFor = (action: DeckAction) => (solo ? undefined : deckKeyHint(action, deck));
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
      className="flex h-(--deck-h) min-w-(--deck-min-w) shrink-0 gap-2 rounded-md border border-border bg-surface p-2 data-[dropping]:border-dashed data-[dropping]:border-accent"
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
        />

        {slot !== null || view === undefined ? (
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
        )}

        <div className="flex h-(--deck-bottom-h) shrink-0 gap-2">
          <DeckTransport
            deck={deck}
            loaded={loaded}
            playing={view?.playing ?? false}
            cueKey={keyFor({ type: "cue" })}
            playKey={playKey === "" ? undefined : playKey}
            onCue={actions.cue}
            onTogglePlay={actions.togglePlay}
          />
          <DeckPads
            deck={deck}
            loaded={loaded}
            grid={grid}
            mode={meta.padMode}
            page={meta.padPage}
            hotCues={meta.hotCues}
            loopBeats={view?.loopRegion?.beats ?? null}
            quantise={loaded && (view?.quantise ?? false)}
            lastJump={meta.lastJump}
            lastLoop={meta.lastLoop}
            keyFor={keyFor}
            onMode={actions.setPadMode}
            onPage={actions.setPadPage}
            onQuantise={actions.toggleQuantise}
            onHotCue={actions.hotCue}
            onJump={actions.beatJump}
            onLoop={actions.loopPad}
          />
          <DeckSyncButtons
            deck={deck}
            solo={solo}
            ready={grid}
            master={view?.master ?? false}
            sync={view?.sync ?? false}
            masterKey={keyFor({ type: "master" })}
            syncKey={keyFor({ type: "sync" })}
            onMaster={actions.takeMaster}
            onSync={actions.toggleSync}
          />
        </div>
      </div>

      <DeckTempo
        deck={deck}
        loaded={loaded}
        tempo={tempo}
        range={meta.tempoRange}
        slowerKey={keyFor({ type: "nudge", dir: -1 })}
        fasterKey={keyFor({ type: "nudge", dir: 1 })}
        onTempo={actions.setTempo}
        onRange={actions.setTempoRange}
        onNudge={actions.nudge}
      />
    </section>
  );
}
