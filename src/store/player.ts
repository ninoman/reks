/**
 * The library's player: one deck of the mix engine (D30), so it has the
 * deck's cue, hot cues, tempo and waveform. Double-click, Space and the
 * history view play on it; the Mix panel shows it with the other decks.
 * While mixing, double-click loads onto a spare deck instead. Track info's
 * "Edit on deck" opens grid mode the same way (`editGridOnDeck`).
 */
import { shownDecks } from "../lib/mixView";
import { spareDeck } from "../lib/spareDeck";
import { useDeckGrid } from "./deckGrid";
import { libraryId, sameTrack, type TrackRef, useMix } from "./mix";
import { usePanels } from "./panels";
import { useToasts } from "./toasts";

/** Deck 2: one of the two decks the Mix panel shows by default (the centre channels). */
export const PLAYER_DECK = 1;

/** Loads `track` on the player deck and plays it (keeps it going if it already is). */
export function playTrack(track: TrackRef): Promise<void> {
  return useMix.getState().load(PLAYER_DECK, track, true);
}

/**
 * Double-click or Enter on a track. Without the Mix panel it plays on the
 * player deck; with it, it loads paused onto a spare deck, never onto one
 * that may be on air.
 */
export function activateTrack(track: TrackRef): Promise<void> {
  if (!usePanels.getState().mixOpen) return playTrack(track);
  const deck = freeDeck();
  return deck === null ? Promise.resolve() : useMix.getState().load(deck, track);
}

/** A spare shown deck while mixing (or says there is none). */
function freeDeck(): number | null {
  const mix = useMix.getState();
  const deck = spareDeck(
    shownDecks(mix.deckCount).map((d) => ({
      deck: d,
      empty: mix.decks[d]?.track == null && mix.decks[d]?.loading == null,
      playing: mix.live?.decks[d]?.playing === true,
      fader: mix.channels[d]?.fader ?? 1,
    })),
  );
  if (deck === null) {
    useToasts.getState().show("No free deck: stop a deck and close its fader to load onto it.");
  }
  return deck;
}

/**
 * Grid mode for library track `track`: on the shown deck that has it, else
 * loaded paused onto the player deck (no Mix panel) or a spare deck.
 */
export async function editGridOnDeck(track: TrackRef & { kind: "library" }): Promise<void> {
  const mixOpen = usePanels.getState().mixOpen;
  const shown = mixOpen ? shownDecks(useMix.getState().deckCount) : [PLAYER_DECK];
  let deck = shown.find((d) => libraryId(useMix.getState().decks[d]?.track) === track.id) ?? null;
  if (deck === null) {
    deck = mixOpen ? freeDeck() : PLAYER_DECK;
    if (deck === null) return;
    await useMix.getState().load(deck, track, false);
  }
  // The load failed or was replaced meanwhile.
  if (libraryId(useMix.getState().decks[deck]?.track) !== track.id) return;
  await useDeckGrid.getState().open(deck, track.id);
}

export function togglePlayer(): void {
  useMix.getState().togglePlay(PLAYER_DECK);
}

/** Library id of the track on the player deck, if any. */
export function usePlayerTrackId(): number | null {
  return useMix((s) => libraryId(s.decks[PLAYER_DECK]?.track));
}

/** The player deck has `ref` loaded (or loading). */
export function usePlayerHas(ref: TrackRef): boolean {
  return useMix((s) => {
    const d = s.decks[PLAYER_DECK];
    return sameTrack(d?.track, ref) || sameTrack(d?.loading, ref);
  });
}

/** The player deck's position while it has library track `id` loaded, else null. */
export function usePlayerPosition(id: number | null): number | null {
  return useMix((s) => {
    if (id === null || libraryId(s.decks[PLAYER_DECK]?.track) !== id) return null;
    const v = s.live?.decks[PLAYER_DECK];
    return v?.loaded === true ? v.positionMs : null;
  });
}
