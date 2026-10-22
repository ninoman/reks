/** A deck's buttons and faders as mix store calls (shared by the Mix panel and the player deck). */
import type { MixStore } from "../../store/mix";
import type { DeckActions } from "./DeckStrip";

export function deckActions(mix: MixStore, deck: number): DeckActions {
  return {
    togglePlay: () => mix.togglePlay(deck),
    play: () => mix.send({ type: "play", deck }),
    pause: () => mix.send({ type: "pause", deck }),
    cue: (down) => mix.cue(deck, down),
    toggleSync: () => mix.toggleSync(deck),
    toggleBar: () => mix.toggleSync(deck, !(mix.live?.decks[deck]?.syncBar ?? false)),
    toggleLeader: () => mix.send({ type: "takeMaster", deck }),
    toggleQuantise: () =>
      mix.send({ type: "quantise", deck, on: !(mix.live?.decks[deck]?.quantise ?? false) }),
    hotCue: (slot) => mix.send({ type: "hotCue", deck, slot }),
    nudge: (dir) => mix.nudge(deck, dir),
    setTempo: (ratio) => mix.setTempo(deck, ratio),
    setTempoRange: (range) => mix.setTempoRange(deck, range),
    zoom: (dir) => mix.zoomDeck(deck, dir),
    seek: (ms) => mix.send({ type: "seek", deck, ms }),
    eject: () => mix.unload(deck),
    drop: (id) => void mix.load(deck, id),
  };
}
