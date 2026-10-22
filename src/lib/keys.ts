/** Keyboard rules shared by the window-wide shortcuts. */

/** Inputs that take no typed text: Space is the player's there. */
const NON_TEXT_INPUTS = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

/** Text fields keep Space for typing; dialogs and menus for their controls. */
export function keepsSpace(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target.tagName === "TEXTAREA") return true;
  if (target instanceof HTMLInputElement && !NON_TEXT_INPUTS.has(target.type)) return true;
  return target.closest('[role="dialog"], [role="alertdialog"], [role="menu"]') !== null;
}

/** The player deck's play/pause key (`KeyboardEvent.code`) and how it reads in a hint. */
export const PLAYER_PLAY_KEY = { code: "Space", label: "Space" } as const;

/**
 * Waveform zoom for every deck, wherever focus is (except text fields,
 * dialogs and menus): − out, + in (the "=" key, Shift or not, and the
 * keypad's). Returns the step, or null for any other key.
 */
export function waveZoomKey(e: {
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  target: EventTarget | null;
}): -1 | 1 | null {
  if (e.metaKey || e.ctrlKey || e.altKey || keepsSpace(e.target)) return null;
  return WAVE_ZOOM[e.code] ?? null;
}

const WAVE_ZOOM: Record<string, -1 | 1> = {
  Minus: -1,
  Equal: 1,
  NumpadSubtract: -1,
  NumpadAdd: 1,
};

/** How the zoom keys read in a hint. */
export const WAVE_ZOOM_KEY = { out: "−", in: "+" } as const;
