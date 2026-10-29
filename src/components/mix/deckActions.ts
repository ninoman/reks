/** A deck's buttons and faders as mix store calls (shared by the Mix panel and the player deck). */
import type { MixStore } from "../../store/mix";
import { syncBar } from "../../store/settings";
import type { DeckActions } from "./DeckStrip";

export function deckActions(mix: MixStore, deck: number): DeckActions {
  return {
    togglePlay: () => mix.togglePlay(deck),
    play: () => mix.send({ type: "play", deck }),
    pause: () => mix.send({ type: "pause", deck }),
    cue: (down) => mix.cue(deck, down),
    toggleSync: () => mix.toggleSync(deck, syncBar()),
    takeMaster: () => mix.takeMaster(deck),
    toggleQuantise: () =>
      mix.send({ type: "quantise", deck, on: !(mix.live?.decks[deck]?.quantise ?? false) }),
    toggleMasterTempo: () =>
      mix.send({ type: "masterTempo", deck, on: !(mix.live?.decks[deck]?.masterTempo ?? false) }),
    hotCuePad: (slot) => mix.hotCuePad(deck, slot),
    editCues: (edit) => void mix.editCues(deck, edit),
    callCue: (ms, endMs) => mix.callCue(deck, ms, endMs),
    beatJump: (beats) => mix.beatJump(deck, beats),
    loopPad: (beats) => mix.loopPad(deck, beats),
    toggleLoop: () => mix.toggleLoop(deck),
    loopIn: () => mix.loopIn(deck),
    loopOut: () => mix.loopOut(deck),
    reloop: () => mix.reloop(deck),
    loopResize: (double) => mix.loopResize(deck, double),
    setPadMode: (mode) => mix.setPadMode(deck, mode),
    setPadPage: (mode, page) => mix.setPadPage(deck, mode, page),
    setLoopPage: (page) => mix.setLoopPage(deck, page),
    nudge: (dir) => mix.nudge(deck, dir),
    setTempo: (ratio) => mix.setTempo(deck, ratio),
    setTempoRange: (range) => mix.setTempoRange(deck, range),
    setBpm: (bpm) => mix.setBpm(deck, bpm),
    zoom: (dir) => mix.zoom(dir),
    seek: (ms, exact = false) => mix.send({ type: "seek", deck, ms, exact }),
    eject: () => mix.unload(deck),
    // A drop onto a playing deck keeps the deck playing with the new track.
    drop: (id) => void mix.load(deck, id, mix.live?.decks[deck]?.playing === true),
    retry: () => void mix.retry(deck),
  };
}
