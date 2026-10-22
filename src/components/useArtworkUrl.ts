/** Artwork bytes from `artwork_get` as an object URL, revoked when the id changes. */
import { useEffect, useState } from "react";

import { artworkGet } from "../lib/ipc";

/** null while loading, without artwork, or when it can't be read. */
export function useArtworkUrl(id: number | null, mime: string): string | null {
  const [loaded, setLoaded] = useState<{ id: number; url: string } | null>(null);

  useEffect(() => {
    if (id === null) return;
    let live = true;
    let created: string | null = null;
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
    return () => {
      live = false;
      if (created !== null) URL.revokeObjectURL(created);
    };
  }, [id, mime]);

  // A stale picture from the previous id is not shown while the new one loads.
  return loaded !== null && loaded.id === id ? loaded.url : null;
}
