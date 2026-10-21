/**
 * Window-wide mixer shortcuts (`src/lib/mixKeys.ts`) while the mixer is
 * open. Text fields, dialogs and menus keep their keys. Nudge keys bend
 * while held and release on key up.
 */
import { useEffect } from "react";

import { keepsSpace } from "../lib/keys";
import { mixKeyAction } from "../lib/mixKeys";
import { useMix } from "./mix";

export function useMixShortcuts(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    /** Held nudge keys: code → deck. */
    const held = new Map<string, number>();

    function onKeyDown(e: KeyboardEvent) {
      if (e.repeat || keepsSpace(e.target)) return;
      const mix = useMix.getState();
      const action = mixKeyAction(e, mix.deckCount);
      if (action === null) return;
      e.preventDefault();
      switch (action.type) {
        case "cue":
          mix.send({ type: "cue", deck: action.deck });
          break;
        case "play":
          mix.togglePlay(action.deck);
          break;
        case "sync":
          mix.toggleSync(action.deck);
          break;
        case "hotCue":
          mix.send({ type: "hotCue", deck: action.deck, slot: action.slot });
          break;
        case "nudge":
          held.set(e.code, action.deck);
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

    function onKeyUp(e: KeyboardEvent) {
      const deck = held.get(e.code);
      if (deck === undefined) return;
      held.delete(e.code);
      useMix.getState().nudge(deck, 0);
    }

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      for (const deck of held.values()) useMix.getState().nudge(deck, 0);
    };
  }, [enabled]);
}
