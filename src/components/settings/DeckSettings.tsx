/** Settings → Decks: the waveform style and what SYNC aligns. */
import type { DeckWaveform, SyncAlign } from "../../lib/ipc";
import { useSettings } from "../../store/settings";
import { ChoiceTile, ChoiceTiles } from "../ui/ChoiceTiles";
import { SyncPreview } from "./previews/SyncPreview";
import { WaveformPreview } from "./previews/WaveformPreview";
import { SettingItem } from "./SettingItem";

const WAVEFORMS: { id: DeckWaveform; label: string; hint: string }[] = [
  { id: "rgb", label: "RGB", hint: "Colour by band" },
  { id: "threeBand", label: "3-band", hint: "Lows, mids and highs" },
  { id: "blue", label: "Blue", hint: "Classic, lighter in the highs" },
];

const SYNC_ALIGNS: { id: SyncAlign; label: string; hint: string }[] = [
  { id: "bar", label: "Bar", hint: "The downbeats as well as the beats." },
  { id: "beat", label: "Beat", hint: "The beats only: the bar can start a beat off." },
];

export function DeckSettings({ waveform, align }: { waveform: DeckWaveform; align: SyncAlign }) {
  const { update } = useSettings.getState();
  return (
    <>
      <SettingItem title="Waveform" hint="How the player deck and the Mix view draw waveforms.">
        <ChoiceTiles
          aria-label="Deck waveform"
          columns={3}
          value={waveform}
          onValueChange={(v) => void update({ deckWaveform: v as DeckWaveform })}
        >
          {WAVEFORMS.map((w) => (
            <ChoiceTile
              key={w.id}
              value={w.id}
              label={w.label}
              hint={w.hint}
              isDefault={w.id === "rgb"}
              preview={<WaveformPreview style={w.id} />}
            />
          ))}
        </ChoiceTiles>
      </SettingItem>
      <SettingItem title="Sync aligns" hint="What SYNC lines up with the master deck.">
        <ChoiceTiles
          aria-label="Sync aligns"
          value={align}
          onValueChange={(v) => void update({ syncAlign: v as SyncAlign })}
        >
          {SYNC_ALIGNS.map((a) => (
            <ChoiceTile
              key={a.id}
              value={a.id}
              label={a.label}
              hint={a.hint}
              isDefault={a.id === "bar"}
              preview={<SyncPreview align={a.id} />}
            />
          ))}
        </ChoiceTiles>
      </SettingItem>
    </>
  );
}
