/**
 * The library's player: one deck of the mix engine (D30), so it has the
 * deck's cue, hot cues, tempo and waveform. Double-click, Space and the
 * history view play on it; the Mix panel shows it with the other decks.
 */
import { type TrackRef, useMix } from "./mix";

/** Deck 2: one of the two decks the Mix panel shows by default (the centre channels). */
export const PLAYER_DECK = 1;

/** Loads `track` on the player deck and plays it (keeps it going if it already is). */
export function playTrack(track: TrackRef): Promise<void> {
  return useMix.getState().load(PLAYER_DECK, track, true);
}

export function togglePlayer(): void {
  useMix.getState().togglePlay(PLAYER_DECK);
}

/** Id of the track on the player deck, if any. */
export function usePlayerTrackId(): number | null {
  return useMix((s) => s.decks[PLAYER_DECK]?.track?.id ?? null);
}

/** The player deck's position while it has track `id` loaded, else null. */
export function usePlayerPosition(id: number | null): number | null {
  return useMix((s) => {
    if (id === null || s.decks[PLAYER_DECK]?.track?.id !== id) return null;
    const v = s.live?.decks[PLAYER_DECK];
    return v?.loaded === true ? v.positionMs : null;
  });
}
