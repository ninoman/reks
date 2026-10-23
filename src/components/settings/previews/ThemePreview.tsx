/** A theme's canvas with a strip of its accent, filling the tile's picture. */
import { cn } from "../../../lib/cn";

export function ThemePreview({ canvas, accent }: { canvas: string; accent: string }) {
  return (
    <span className={cn("absolute inset-0 flex items-end gap-1.5 p-2", canvas)}>
      <span className={cn("h-1.5 w-1/2 rounded-full", accent)} />
      <span className={cn("size-1.5 rounded-full", accent)} />
    </span>
  );
}
