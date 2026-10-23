/**
 * The library's player when the Mix panel is off (1 deck): the player deck
 * alone, in the same deck design as the Mix view (D30) but laid out flat
 * (`DeckStrip` `solo`). Connects the mix store to the presentational
 * `DeckStrip`.
 */
import { useMix } from "../store/mix";
import { playerDeviations } from "../store/mixDeviations";
import { usePanels } from "../store/panels";
import { PLAYER_DECK } from "../store/player";
import { useKeyLabel } from "../store/settings";
import { DeckStrip } from "./mix/DeckStrip";
import { deckActions } from "./mix/deckActions";
import { Chip } from "./ui/Chip";

export function PlayerDeck() {
  const mix = useMix();
  const keyLabel = useKeyLabel();
  const deviations = playerDeviations(mix);
  const meta = mix.decks[PLAYER_DECK];
  if (meta === undefined) return null;
  const view = mix.status === "running" ? mix.live?.decks[PLAYER_DECK] : undefined;

  return (
    <div className="shrink-0 px-3 pb-2">
      <DeckStrip
        deck={PLAYER_DECK}
        view={view}
        meta={meta}
        zoom={mix.waveZoom}
        keyText={meta.track?.key == null ? null : keyLabel(meta.track.key)}
        phase={null}
        solo
        notice={
          deviations.length > 0 && (
            <Chip
              tone="notice"
              className="flex-none"
              onActivate={() => {
                usePanels.getState().setMixOpen(true);
                useMix.getState().setDeckCount(2);
              }}
              activateLabel={`${deviations.join(", ")}: show the mixer (2 decks)`}
            >
              {deviations[0]}
              {deviations.length > 1 && ` +${deviations.length - 1}`}
            </Chip>
          )
        }
        actions={deckActions(mix, PLAYER_DECK)}
      />
    </div>
  );
}
