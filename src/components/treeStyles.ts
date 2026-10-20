/** Row styles shared by the sidebar trees (playlists, histories). */

/** One tree row; state comes in as data attributes (cheap, no class building). */
export const ROW = [
  "flex h-(--row-h) cursor-default items-center gap-1.5 whitespace-nowrap pr-2 pl-1 text-md text-fg",
  "data-[root]:text-fg-muted data-[selected]:bg-selected data-[selected]:text-selected-fg",
  "data-[drop=into]:shadow-[inset_0_0_0_2px_var(--color-accent)]",
  "data-[drop=before]:shadow-[inset_0_2px_0_var(--color-accent)]",
  "data-[drop=after]:shadow-[inset_0_-2px_0_var(--color-accent)]",
].join(" ");
export const DISCLOSURE = "inline-flex w-3.5 flex-none justify-center text-fg-muted";
export const ICON = "flex-none text-fg-muted";
export const NAME = "flex-1 overflow-hidden text-ellipsis";

/** Indent per level and the row's own left padding, in px (rekordbox-like spacing). */
export const INDENT = 18;
export const PAD = 4;
