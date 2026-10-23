/** Which deck a library double-click loads onto while mixing. */

/** What the rule needs to know about one deck on screen. */
export interface DeckOccupancy {
  deck: number;
  /** No track loaded or loading. */
  empty: boolean;
  playing: boolean;
  /** Channel fader, 0..1. */
  fader: number;
}

/**
 * A deck that can take a track without interrupting the mix: an empty one
 * first, else the lowest-numbered deck that is stopped with its fader fully
 * closed. Null when every deck may be on air.
 */
export function spareDeck(decks: readonly DeckOccupancy[]): number | null {
  const byNumber = [...decks].sort((a, b) => a.deck - b.deck);
  const pick = byNumber.find((d) => d.empty) ?? byNumber.find((d) => !d.playing && d.fader <= 0);
  return pick?.deck ?? null;
}
