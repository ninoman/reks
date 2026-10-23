/**
 * What the player and Mix panel cannot show (pure): decks that play while
 * their strip is off screen, and the player deck's mixer settings (channel 2
 * and the master) that are off their defaults in 1-deck mode, where the
 * mixer is hidden.
 */
import type { ChannelValues, FilterValues, MasterValues } from "../store/mix";
import { FX_DEAD_ZONE, offCentre } from "./mixView";

/** Decks on screen: the player deck alone, deck 2 and 3 with two, all four. */
function shown(deck: number, mixOpen: boolean, deckCount: 2 | 4): boolean {
  if (!mixOpen) return deck === 1;
  return deckCount === 4 || deck === 1 || deck === 2;
}

/** Decks (0-based) that play without being shown, in deck order. */
export function hiddenPlaying(
  playing: readonly boolean[],
  mixOpen: boolean,
  deckCount: 2 | 4,
): number[] {
  return playing.flatMap((p, d) => (p && !shown(d, mixOpen, deckCount) ? [d] : []));
}

/** "Deck 1 playing", "Decks 1, 4 playing". */
export function hiddenPlayingLabel(decks: readonly number[]): string {
  const nums = decks.map((d) => d + 1).join(", ");
  return `${decks.length === 1 ? "Deck" : "Decks"} ${nums} playing`;
}

const EQ_KEYS = ["eqHi", "eqMid1", "eqMid2", "eqLo", "eq3Hi", "eq3Mid", "eq3Lo"] as const;

/**
 * Why the sound of the player deck (channel 2) differs from a flat mixer,
 * most audible first; empty when every setting that reaches it is at its
 * default. `channel` and `filters` are what the engine gets (`effectiveMix`);
 * `reference` holds the defaults.
 */
export function mixerDeviations(
  channel: ChannelValues,
  filters: Record<"x" | "y", FilterValues>,
  master: MasterValues,
  reference: { channel: ChannelValues; master: MasterValues },
): string[] {
  const out: string[] = [];
  const c = reference.channel;
  if (channel.fader !== c.fader) out.push("Channel 2 fader not up");
  if (EQ_KEYS.some((k) => channel[k] !== c[k])) out.push("Channel 2 EQ not flat");
  if (channel.trim !== c.trim) out.push("Channel 2 trim not centred");
  if (channel.filterAssign !== "off" && filters[channel.filterAssign].on) {
    out.push("Channel 2 filter on");
  }
  if (offCentre(channel.fx, FX_DEAD_ZONE)) out.push("Channel 2 FX on");
  if (master.level !== reference.master.level) out.push("Master level changed");
  return out;
}
