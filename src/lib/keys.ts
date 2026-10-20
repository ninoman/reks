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
