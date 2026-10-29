/**
 * Grid mode's keys and focus (D10, phase 5 task 32). A press on a deck
 * focuses it; a press anywhere else clears the focus. The grid keys
 * (`gridKey`) act only while the deck in grid mode has the focus, and never
 * in text fields, dialogs or menus; they go first (capture), so the
 * overview's own arrow-key seek and other keys don't also take them. Grid
 * mode closes when its deck gets
 * another track, empties, or is no longer shown.
 */
import { useEffect } from "react";

import { gridKey } from "../lib/gridMode";
import { keepsSpace } from "../lib/keys";
import { shownDecks } from "../lib/mixView";
import { useDeckGrid } from "./deckGrid";
import { libraryId, useMix } from "./mix";
import { usePanels } from "./panels";
import { PLAYER_DECK } from "./player";

export function useDeckGridShortcuts() {
  useEffect(() => {
    function onPointerDown(e: PointerEvent) {
      const el = e.target instanceof Element ? e.target.closest("[data-deck]") : null;
      const deck = el === null ? null : Number(el.getAttribute("data-deck"));
      useDeckGrid.getState().setFocus(deck);
    }

    function onKeyDown(e: KeyboardEvent) {
      const g = useDeckGrid.getState();
      if (g.deck === null || g.focus !== g.deck || keepsSpace(e.target)) return;
      const action = gridKey(e);
      if (action === null) return;
      e.preventDefault();
      e.stopPropagation();
      const at = useMix.getState().live?.decks[g.deck]?.positionMs ?? 0;
      switch (action.type) {
        case "shift":
          g.shift(at, action.dir, action.phase, action.big);
          break;
        case "bpm":
          g.bpmStep(at, action.dir, action.phase, action.big);
          break;
        case "beatOne":
          g.beatOne(at, !action.exact);
          break;
        case "stretch":
          g.stretch(at, !action.exact);
          break;
        case "marker":
          g.marker(at, !action.exact);
          break;
        case "halve":
          g.halve(at);
          break;
        case "double":
          g.double(at);
          break;
        case "undo":
          g.undo();
          break;
        case "redo":
          g.redo();
          break;
        case "exit":
          g.close();
          break;
      }
    }

    function closeIfGone() {
      const g = useDeckGrid.getState();
      if (g.deck === null) return;
      const mix = useMix.getState();
      const meta = mix.decks[g.deck];
      const shown = usePanels.getState().mixOpen ? shownDecks(mix.deckCount) : [PLAYER_DECK];
      if (!shown.includes(g.deck) || libraryId(meta?.track) !== g.trackId) g.close();
    }

    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown, true);
    const stopMix = useMix.subscribe(closeIfGone);
    const stopPanels = usePanels.subscribe(closeIfGone);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown, true);
      stopMix();
      stopPanels();
    };
  }, []);
}
