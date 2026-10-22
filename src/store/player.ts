/**
 * The library's player: one deck of the mix engine (D30), so it has the
 * deck's cue, hot cues, tempo and waveform. Double-click, Space and the
 * history view play on it; the Mix panel shows it with the other decks.
 */
import { libraryId, sameTrack, type TrackRef, useMix } from "./mix";

/** Deck 2: one of the two decks the Mix panel shows by default (the centre channels). */
export const PLAYER_DECK = 1;

/** Loads `track` on the player deck and plays it (keeps it going if it already is). */
export function playTrack(track: TrackRef): Promise<void> {
  return useMix.getState().load(PLAYER_DECK, track, true);
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
