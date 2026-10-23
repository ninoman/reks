/**
 * A track row's artwork; clicking it plays the track on the preview deck
 * (`store/preview.ts`) or pauses it there. A plain button with a constant class, not `IconButton`: it
 * is in every row (docs/ui.md, track table performance rule).
 */
import type { TrackRow } from "../lib/ipc";
import { togglePreview, usePreview, usePreviewOf } from "../store/preview";
import { PauseIcon, PlayIcon } from "./icons";
import { Artwork } from "./ui/Artwork";

const PLAY =
  "flex size-full cursor-pointer items-center justify-center rounded-xs bg-overlay text-deck-fg outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--color-focus-ring)] disabled:cursor-default [&_svg]:size-3/5";

export function QuickPlayArtwork({ row }: { row: TrackRow }) {
  const playing = usePreviewOf(row.id)?.playing === true;
  const loading = usePreview((s) => s.loadingId === row.id);
  const verb = playing ? "Pause" : "Preview";
  return (
    <Artwork
      size="row"
      artworkId={row.artworkId}
      actionShown={playing || loading}
      action={
        <button
          type="button"
          className={PLAY}
          aria-label={`${verb} ${row.title ?? "track"}`}
          aria-busy={loading || undefined}
          title={playing ? "Pause preview" : "Preview (headphones when routed)"}
          disabled={row.missing}
          // The row would select or start a drag, or load the player on a double-click.
          onMouseDown={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
          onClick={() => void togglePreview(row.id)}
        >
          {playing ? <PauseIcon /> : <PlayIcon />}
        </button>
      }
    />
  );
}
