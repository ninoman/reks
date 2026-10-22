/**
 * A deck's header row: deck number (lit when loaded), artwork (eject on
 * hover or focus), title and artist, key, BPM (double-click to type one)
 * with the tempo under it, and remaining over elapsed time.
 */
import { formatClock, formatTempo } from "../../lib/mixView";
import type { DeckTrack, TrackRef } from "../../store/mix";
import { ArtworkThumb } from "../ArtworkThumb";
import { EjectIcon } from "../icons";
import { Badge } from "../ui/Badge";
import { IconButton } from "../ui/IconButton";
import { InlineNumber } from "../ui/InlineNumber";

export interface DeckHeaderProps {
  deck: number;
  loaded: boolean;
  track: DeckTrack | null;
  loading: TrackRef | null;
  /** The key in the chosen notation, if the track has one. */
  keyText: string | null;
  /** Tempo heard at the playhead; null without a grid. */
  bpm: number | null;
  tempo: number;
  /** Why the BPM can't be typed, or null when it can. */
  bpmLocked: string | null;
  positionMs: number;
  durationMs: number;
  onBpm: (bpm: number) => void;
  onEject: () => void;
}

export function DeckHeader({
  deck,
  loaded,
  track,
  loading,
  keyText,
  bpm,
  tempo,
  bpmLocked,
  positionMs,
  durationMs,
  onBpm,
  onEject,
}: DeckHeaderProps) {
  const n = deck + 1;
  const shown = track ?? loading;
  return (
    <header className="flex h-(--deck-header-h) min-w-0 shrink-0 items-center gap-1.5">
      <Badge variant={loaded ? "accent" : "muted"} size="md" className="rounded-xs font-mono">
        {n}
      </Badge>
      <ArtworkThumb
        size="deck"
        artworkId={track?.artworkId ?? null}
        mime={track?.artworkMime ?? ""}
        action={
          loaded ? (
            <IconButton
              aria-label={`Eject deck ${n}`}
              hint="Eject"
              className="size-full rounded-xs bg-overlay text-deck-fg hover:bg-overlay hover:text-deck-fg"
              onClick={onEject}
            >
              <EjectIcon />
            </IconButton>
          ) : undefined
        }
      />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        {track !== null ? (
          <span className="truncate text-sm font-semibold">{track.title ?? "Untitled"}</span>
        ) : loading !== null ? (
          <span className="truncate text-sm text-fg-muted">
            Loading {loading.title ?? "track"}…
          </span>
        ) : (
          <span className="text-sm text-fg-subtle">Empty</span>
        )}
        <span className="truncate text-xs text-fg-muted">
          {shown === null ? `Deck ${n}` : (shown.artist ?? "")}
        </span>
      </div>
      <Badge
        variant={keyText === null ? "muted" : "neutral"}
        size="md"
        className="min-w-8 rounded-xs font-mono"
        title="Key"
      >
        {keyText ?? "—"}
      </Badge>
      <div className="flex w-18 shrink-0 flex-col items-end gap-px font-mono tabular-nums">
        <InlineNumber
          value={bpm}
          format={(v) => v.toFixed(2)}
          onCommit={onBpm}
          disabled={bpmLocked !== null}
          aria-label={`Deck ${n} BPM`}
          title={bpmLocked ?? "Double-click to type a BPM"}
          className="text-lg leading-tight font-bold tracking-tight"
        />
        <span className="pr-0.5 text-2xs text-fg-muted">{formatTempo(tempo)}</span>
      </div>
      <div className="flex w-15 shrink-0 flex-col items-end gap-0.5 font-mono tabular-nums">
        <span className="text-xs font-bold">
          {formatClock(Math.max(0, durationMs - positionMs), true)}
        </span>
        <span className="text-2xs text-fg-muted">{formatClock(positionMs)}</span>
      </div>
    </header>
  );
}
