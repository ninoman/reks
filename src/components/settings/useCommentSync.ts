/**
 * "Update all comments now" (D36): how many tagged tracks' comment blocks
 * differ from their tags (`comment_tags_sync_all` dry run), and rewriting them.
 */
import { useCallback, useEffect, useState } from "react";

import { commentTagsSyncAll } from "../../lib/ipc";
import { refreshAll } from "../../store/panes";

export interface CommentSync {
  /** `null` while counting or when off. */
  pending: number | null;
  busy: boolean;
  error: string | null;
  apply(): Promise<void>;
}

export function useCommentSync(on: boolean): CommentSync {
  const [pending, setPending] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const count = useCallback(async () => {
    setPending(null);
    try {
      setPending(await commentTagsSyncAll(false));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    if (on) void count();
    else setPending(null);
  }, [on, count]);

  async function apply() {
    setBusy(true);
    setError(null);
    try {
      if ((await commentTagsSyncAll(true)) > 0) refreshAll(true);
      await count();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return { pending, busy, error, apply };
}
