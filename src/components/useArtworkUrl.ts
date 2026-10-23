/**
 * Artwork bytes from `artwork_get` (or the small `artwork_thumb` JPEG) as an
 * object URL. Thumbnails of a list are shared through a bounded cache so a
 * scroll back over rows costs no IPC; full-size URLs are revoked when the id
 * changes.
 */
import { useEffect, useState } from "react";

import { artworkGet, artworkThumb } from "../lib/ipc";

const THUMB_CACHE_MAX = 600;
/** id → URL (null: unreadable, so rows don't ask again). Map order = recency. */
const thumbs = new Map<number, Promise<string | null>>();

function cachedThumb(id: number): Promise<string | null> {
  const hit = thumbs.get(id);
  if (hit !== undefined) {
    thumbs.delete(id);
    thumbs.set(id, hit);
    return hit;
  }
  const url = artworkThumb(id).then(
    (bytes) => URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })),
    () => null,
  );
  thumbs.set(id, url);
  if (thumbs.size > THUMB_CACHE_MAX) {
    const [oldest] = thumbs.keys();
    if (oldest !== undefined) {
      thumbs.get(oldest)?.then((u) => u !== null && URL.revokeObjectURL(u));
      thumbs.delete(oldest);
    }
  }
  return url;
}

/** Test hook: forget every cached thumbnail. */
export function clearThumbCache(): void {
  for (const p of thumbs.values()) p.then((u) => u !== null && URL.revokeObjectURL(u));
  thumbs.clear();
}

/** null while loading, without artwork, or when it can't be read. */
export function useArtworkUrl(id: number | null, mime: string, thumb = false): string | null {
  const [loaded, setLoaded] = useState<{ id: number; url: string } | null>(null);

  useEffect(() => {
    if (id === null) return;
    let live = true;
    let created: string | null = null;
    if (thumb) {
      cachedThumb(id).then((url) => {
        if (live && url !== null) setLoaded({ id, url });
      });
    } else {
      artworkGet(id).then(
        (bytes) => {
          if (!live) return;
          created = URL.createObjectURL(new Blob([bytes], { type: mime }));
          setLoaded({ id, url: created });
        },
        () => {
          // A deleted cache file: show the placeholder.
        },
      );
    }
    return () => {
      live = false;
      if (created !== null) URL.revokeObjectURL(created);
    };
  }, [id, mime, thumb]);

  // A stale picture from the previous id is not shown while the new one loads.
  return loaded !== null && loaded.id === id ? loaded.url : null;
}
