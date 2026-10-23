/**
 * The mix-prep panel (phase 5), under the player bar at the top of the
 * window: decks on either side of the mixer, 1–2 on the left and 3–4 on the
 * right; with two shown, decks 2 and 3. Connects the mixer store to the
 * presentational deck and mixer components. Shown whether the output runs
 * or not: loading a track opens it, and the mixer's positions reach the
 * engine when it starts. Its controls (deck count, shortcuts, start / stop)
 * and status line, errors included, are in the player bar (`PlayerBar`).
 */
import { memo, useMemo } from "react";

import { deckSide, mixerSize, phaseOffset, shownDecks } from "../../lib/mixView";
import { useMix } from "../../store/mix";
import { useKeyLabel } from "../../store/settings";
import { DeckStrip } from "./DeckStrip";
import { deckActions } from "./deckActions";
import { MixerPanel } from "./MixerPanel";

/** The mixer does not depend on the 60 Hz engine state: skip those renders. */
const Mixer = memo(MixerPanel);

export function MixPanel() {
  const mix = useMix();
  const { live, deckCount } = mix;
  const shown = shownDecks(deckCount);
  const keyLabel = useKeyLabel();
  const activeKey = mix.decks.map((d) => (d.track === null ? 0 : 1)).join("");
  // Recomputed only when a deck loads or empties.
  const active = useMemo(() => [...activeKey].map((c) => c === "1"), [activeKey]);

  function strip(deck: number) {
    const view = live?.decks[deck];
    const meta = mix.decks[deck];
    if (meta === undefined) return null;
    const master = live?.masterDeck === deck;
    const masterBeat =
      live?.masterDeck == null ? null : (live.decks[live.masterDeck]?.beat ?? null);
    const phase =
      !master && view?.beat != null && masterBeat !== null
        ? phaseOffset(view.beat, masterBeat, view.syncBar ? 4 : 1)
        : null;
    const key = meta.track?.key ?? null;
    return (
      <DeckStrip
        key={deck}
        deck={deck}
        view={view}
        meta={meta}
        zoom={mix.waveZoom}
        keyText={key === null ? null : keyLabel(key)}
        phase={phase}
        compact={deckCount === 4}
        actions={deckActions(mix, deck)}
      />
    );
  }

  return (
    <section
      aria-label="Mix"
      data-mixer={mixerSize(mix.look)}
      data-decks={deckCount}
      className="max-h-(--mix-panel-max-h) shrink-0 overflow-y-auto px-3 pb-2"
    >
      <div className="flex min-h-0 items-start gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          {shown.filter((d) => deckSide(d) === "left").map(strip)}
        </div>
        <Mixer
          channels={mix.channels}
          active={active}
          filters={mix.filters}
          crossfader={mix.crossfader}
          fx={mix.fx}
          master={mix.master}
          look={mix.look}
          onChannel={mix.setChannel}
          onFilter={mix.setFilter}
          onCrossfader={mix.setCrossfader}
          onFx={mix.setFx}
          onMaster={mix.setMaster}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          {shown.filter((d) => deckSide(d) === "right").map(strip)}
        </div>
      </div>
    </section>
  );
}
