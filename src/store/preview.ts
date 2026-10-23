/**
 * Quick play from the track list on the preview deck (`PREVIEW_DECK`): in
 * the headphone cue when the output has one, else on the master with the
 * mix faded out while it plays (reks-mix). It never touches a mixer deck.
 */
import { create } from "zustand";

import { type DeckView, mixControl, mixLoad, PREVIEW_DECK } from "../lib/ipc";
import { useMix } from "./mix";
import { useToasts } from "./toasts";

interface PreviewState {
  /** The track being decoded for the preview deck. */
  loadingId: number | null;
}

export const usePreview = create<PreviewState>()(() => ({ loadingId: null }));

/** Bumped per load, so a slow decode doesn't win over a newer click. */
let seq = 0;

function view(): DeckView | undefined {
  return useMix.getState().live?.preview;
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Loads `id` on the preview deck (starting the output if needed); false if it didn't. */
async function load(id: number): Promise<boolean> {
  if (view()?.trackId === id) return true;
  const mine = ++seq;
  usePreview.setState({ loadingId: id });
  try {
    const mix = useMix.getState();
    if (mix.status !== "running" && !(await mix.start())) return false;
    const loaded = await mixLoad(PREVIEW_DECK, id);
    return loaded !== null && mine === seq;
  } catch (e) {
    useToasts.getState().show(`Couldn't play the track: ${message(e)}`);
    return false;
  } finally {
    if (mine === seq) usePreview.setState({ loadingId: null });
  }
}

/** Plays `id` from the start, or pauses / resumes it when it is already on the preview deck. */
export async function togglePreview(id: number): Promise<void> {
  const v = view();
  if (v?.trackId === id && v.loaded) {
    await mixControl({ type: v.playing ? "pause" : "play", deck: PREVIEW_DECK });
    return;
  }
  if (await load(id)) await mixControl({ type: "play", deck: PREVIEW_DECK });
}

/** Plays `id` from `ms` (a click on its waveform), loading it first if needed. */
export async function seekPreview(id: number, ms: number): Promise<void> {
  if (!(await load(id))) return;
  await mixControl({ type: "seek", deck: PREVIEW_DECK, ms });
  if (view()?.playing !== true) await mixControl({ type: "play", deck: PREVIEW_DECK });
}

/** The preview deck while it has track `id`, else null. */
export function usePreviewOf(id: number | null): DeckView | null {
  return useMix((s) => {
    const v = s.live?.preview;
    return id !== null && v?.trackId === id && v.loaded ? v : null;
  });
}
