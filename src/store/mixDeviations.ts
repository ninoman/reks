/** The player deck's mixer settings that are off their defaults (`lib/mixHidden`). */
import { mixerDeviations } from "../lib/mixHidden";
import { DEFAULT_CHANNEL, DEFAULT_MASTER, type MixStore } from "./mix";
import { PLAYER_DECK } from "./player";

/** Reasons channel 2 plays quieter than it could, in a store snapshot. */
export function playerDeviations(s: Pick<MixStore, "channels" | "master">): string[] {
  const channel = s.channels[PLAYER_DECK];
  if (channel === undefined) return [];
  return mixerDeviations(channel, s.master, { channel: DEFAULT_CHANNEL, master: DEFAULT_MASTER });
}
