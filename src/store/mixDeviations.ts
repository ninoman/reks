/** The player deck's mixer settings that are off their defaults (`lib/mixHidden`). */
import { mixerDeviations } from "../lib/mixHidden";
import { effectiveMix } from "../lib/mixView";
import { DEFAULT_CHANNEL, DEFAULT_MASTER, type MixStore } from "./mix";
import { PLAYER_DECK } from "./player";

/** Reasons channel 2 or the master do not sound flat, in a store snapshot, as the engine gets it. */
export function playerDeviations(
  s: Pick<MixStore, "channels" | "filters" | "master" | "look">,
): string[] {
  // What the engine plays: the other EQ layout's knobs and hidden controls do not count.
  const { channels, filters } = effectiveMix(s, s.look);
  const channel = channels[PLAYER_DECK];
  if (channel === undefined) return [];
  return mixerDeviations(channel, filters, s.master, {
    channel: DEFAULT_CHANNEL,
    master: DEFAULT_MASTER,
  });
}
