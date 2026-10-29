/**
 * What the player and Mix panel cannot show (pure): decks that play while
 * their strip is off screen, and the player deck's fader and master level
 * when off their defaults in 1-deck mode, whose one-channel mixer lacks them.
 */
import type { ChannelValues, MasterValues } from "../store/mix";

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

/**
 * Why the player deck (channel 2) sounds quieter than it could in 1-deck
 * mode, where the one-channel mixer has no fader or master section; empty
 * when both are at their defaults. `reference` holds the defaults.
 */
export function mixerDeviations(
  channel: Pick<ChannelValues, "fader">,
  master: MasterValues,
  reference: { channel: Pick<ChannelValues, "fader">; master: MasterValues },
): string[] {
  const out: string[] = [];
  if (channel.fader !== reference.channel.fader) out.push("Channel 2 fader not up");
  if (master.level !== reference.master.level) out.push("Master level changed");
  return out;
}
