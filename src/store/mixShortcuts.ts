/**
 * Window-wide mixer shortcuts (`src/lib/mixKeys.ts`): the Mix panel's decks,
 * or the player deck alone (left-hand keys) with the panel closed. Text fields, dialogs and menus keep their keys. Nudge keys bend
 * while held and CUE previews while held; both release on key up.
 */
import { useEffect } from "react";

import { keepsSpace } from "../lib/keys";
import { mixKeyAction } from "../lib/mixKeys";
import { mixerParts } from "../lib/mixView";
import { useMix } from "./mix";
import { usePanels } from "./panels";

export function useMixShortcuts() {
  useEffect(() => {
    /** Held nudge and cue keys: code → what to release. */
    const held = new Map<string, { type: "nudge" | "cue"; deck: number }>();

    function onKeyDown(e: KeyboardEvent) {
      if (e.repeat || keepsSpace(e.target)) return;
      const mix = useMix.getState();
      const action = mixKeyAction(
        e,
        usePanels.getState().mixOpen ? mix.deckCount : 1,
        mixerParts(mix.look).crossfader,
      );
      if (action === null) return;
      e.preventDefault();
      switch (action.type) {
        case "cue":
          held.set(e.code, { type: "cue", deck: action.deck });
          mix.cue(action.deck, true);
          break;
        case "play":
          mix.togglePlay(action.deck);
          break;
        case "sync":
          mix.toggleSync(action.deck);
          break;
        case "jump":
          mix.jumpLast(action.deck, action.dir);
          break;
        case "loop":
          mix.toggleLoop(action.deck);
          break;
        case "loopIn":
          mix.loopIn(action.deck);
          break;
        case "loopOut":
          mix.loopOut(action.deck);
          break;
        case "reloop":
          mix.reloop(action.deck);
          break;
        case "loopResize":
          mix.loopResize(action.deck, action.double);
          break;
        case "master":
          mix.takeMaster(action.deck);
          break;
        case "padMode":
          mix.cyclePadMode(action.deck);
          break;
        case "hotCue":
          mix.hotCuePad(action.deck, action.slot);
          break;
        case "memoryCue":
          void mix.editCues(action.deck, { type: "addMemoryCue" });
          break;
        case "callCue":
          mix.callAdjacentCue(action.deck, action.dir);
          break;
        case "nudge":
          held.set(e.code, { type: "nudge", deck: action.deck });
          mix.nudge(action.deck, action.dir);
          break;
        case "crossfader": {
          const pos = mix.crossfader.pos;
          const next = action.step === 0 ? 0.5 : Math.min(1, Math.max(0, pos + action.step * 0.1));
          mix.setCrossfader({ pos: next });
          break;
        }
      }
    }

    function release(h: { type: "nudge" | "cue"; deck: number }) {
      const mix = useMix.getState();
      if (h.type === "cue") mix.cue(h.deck, false);
      else mix.nudge(h.deck, 0);
    }

    function onKeyUp(e: KeyboardEvent) {
      const h = held.get(e.code);
      if (h === undefined) return;
      held.delete(e.code);
      release(h);
    }

    // Keys let go while the window is in the background never send key up.
    function onBlur() {
      for (const h of held.values()) release(h);
      held.clear();
    }

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
      onBlur();
    };
  }, []);
}
