/**
 * Handlers for a button that steps once when pressed and repeats while held
 * (grid mode's SHIFT and BPM − / +): `onStep("press", shift)` on pointer down
 * or Enter / Space, then `onStep("repeat", shift)` every `HOLD_REPEAT_MS`
 * after `HOLD_DELAY_MS`, until it is let go or the pointer leaves.
 */
import { type KeyboardEvent, type PointerEvent, useEffect, useRef } from "react";

import { HOLD_DELAY_MS, HOLD_REPEAT_MS, type HoldPhase } from "../../lib/gridMode";

export function useHoldRepeat(onStep: (phase: HoldPhase, shift: boolean) => void) {
  const step = useRef(onStep);
  step.current = onStep;
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  function stop() {
    clearTimeout(timer.current);
    timer.current = undefined;
  }
  useEffect(() => () => clearTimeout(timer.current), []);

  function start(shift: boolean) {
    stop();
    step.current("press", shift);
    const repeat = () => {
      step.current("repeat", shift);
      timer.current = setTimeout(repeat, HOLD_REPEAT_MS);
    };
    timer.current = setTimeout(repeat, HOLD_DELAY_MS);
  }

  const isPressKey = (e: KeyboardEvent) => e.key === "Enter" || e.key === " ";

  return {
    onPointerDown: (e: PointerEvent) => {
      if (e.button === 0) start(e.shiftKey);
    },
    onPointerUp: stop,
    onPointerCancel: stop,
    onPointerLeave: stop,
    onKeyDown: (e: KeyboardEvent) => {
      if (!isPressKey(e)) return;
      e.preventDefault();
      if (!e.repeat) start(e.shiftKey);
    },
    onKeyUp: (e: KeyboardEvent) => {
      if (isPressKey(e)) stop();
    },
  };
}
