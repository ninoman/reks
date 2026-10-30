/** Settings → Decks: the waveform style. */
import type { DeckWaveform } from "../../lib/ipc";
import { useSettings } from "../../store/settings";
import { ChoiceTile, ChoiceTiles } from "../ui/ChoiceTiles";
import { WaveformPreview } from "./previews/WaveformPreview";
import { SettingItem } from "./SettingItem";

const WAVEFORMS: { id: DeckWaveform; label: string; hint: string }[] = [
  { id: "rgb", label: "RGB", hint: "Colour by band" },
  { id: "threeBand", label: "3-band", hint: "Lows, mids and highs" },
  { id: "blue", label: "Blue", hint: "Classic, lighter in the highs" },
];

export function DeckSettings({ waveform }: { waveform: DeckWaveform }) {
  const { update } = useSettings.getState();
  return (
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
  );
}
