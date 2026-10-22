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
    hotCue: (slot) => mix.send({ type: "hotCue", deck, slot }),
    beatJump: (beats) => mix.beatJump(deck, beats),
    loopPad: (beats) => mix.loopPad(deck, beats),
    setPadMode: (mode) => mix.setPadMode(deck, mode),
    setPadPage: (mode, page) => mix.setPadPage(deck, mode, page),
    nudge: (dir) => mix.nudge(deck, dir),
    setTempo: (ratio) => mix.setTempo(deck, ratio),
    setTempoRange: (range) => mix.setTempoRange(deck, range),
    setBpm: (bpm) => mix.setBpm(deck, bpm),
    zoom: (dir) => mix.zoom(dir),
    seek: (ms) => mix.send({ type: "seek", deck, ms }),
    eject: () => mix.unload(deck),
    drop: (id) => void mix.load(deck, id),
    retry: () => void mix.retry(deck),
  };
}
