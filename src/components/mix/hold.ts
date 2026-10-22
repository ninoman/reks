/**
 * Handlers for a button that acts while held (CUE, nudge): pointer down/up
 * and Enter/Space down/up, released when the pointer leaves while pressed.
 */
import type { KeyboardEvent, PointerEvent } from "react";

const isPressKey = (e: KeyboardEvent) => e.key === "Enter" || e.key === " ";

export function holdHandlers(press: (down: boolean) => void) {
  return {
    onPointerDown: (e: PointerEvent) => {
      if (e.button === 0) press(true);
    },
    onPointerUp: () => press(false),
    onPointerCancel: () => press(false),
    onPointerLeave: (e: PointerEvent) => {
      if (e.buttons !== 0) press(false);
    },
    onKeyDown: (e: KeyboardEvent) => {
      if (isPressKey(e) && !e.repeat) {
        e.preventDefault();
        press(true);
      }
    },
    onKeyUp: (e: KeyboardEvent) => {
      if (isPressKey(e)) press(false);
    },
  };
}
