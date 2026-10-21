/**
 * The library's player when the Mix panel is off: the player deck alone,
 * in the same deck design as the Mix view (D30). Connects the mix store to
 * the presentational `DeckStrip`.
 */
import { useMix } from "../store/mix";
import { PLAYER_DECK } from "../store/player";
import { DeckStrip } from "./mix/DeckStrip";
import { deckActions } from "./mix/deckActions";

export function PlayerDeck() {
  const mix = useMix();
  const meta = mix.decks[PLAYER_DECK];
  if (meta === undefined) return null;
  const view = mix.status === "running" ? mix.live?.decks[PLAYER_DECK] : undefined;

  return (
    <div className="shrink-0 bg-bg-subtle px-3 pb-2">
      <DeckStrip
        deck={PLAYER_DECK}
        view={view}
        meta={meta}
        leader={false}
        leaderChosen={false}
        phase={null}
        compact={false}
        solo
        actions={deckActions(mix, PLAYER_DECK)}
      />
    </div>
  );
}
