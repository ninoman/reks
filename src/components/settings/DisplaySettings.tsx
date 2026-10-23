/** Settings → Display: the theme and the key notation. */
import type { KeyNotation, Theme } from "../../lib/ipc";
import { useSettings } from "../../store/settings";
import { ChoiceTile, ChoiceTiles } from "../ui/ChoiceTiles";
import { ThemePreview } from "./previews/ThemePreview";
import { SettingItem } from "./SettingItem";

/** Each theme with its canvas and accent colours (tokens.css `--color-theme-*`). */
const THEMES: { id: Theme; label: string; hint: string; canvas: string; accent: string }[] = [
  {
    id: "graphite",
    label: "Graphite",
    hint: "Warm dark grey",
    canvas: "bg-theme-graphite",
    accent: "bg-theme-graphite-accent",
  },
  {
    id: "paper",
    label: "Paper",
    hint: "Light",
    canvas: "bg-theme-paper",
    accent: "bg-theme-paper-accent",
  },
  {
    id: "midnight",
    label: "Midnight",
    hint: "Navy",
    canvas: "bg-theme-midnight",
    accent: "bg-theme-midnight-accent",
  },
  {
    id: "dusk",
    label: "Dusk",
    hint: "Plum",
    canvas: "bg-theme-dusk",
    accent: "bg-theme-dusk-accent",
  },
  {
    id: "booth",
    label: "Booth",
    hint: "Black, most contrast",
    canvas: "bg-theme-booth",
    accent: "bg-theme-booth-accent",
  },
];

const NOTATIONS: { id: KeyNotation; label: string; example: string }[] = [
  { id: "classical", label: "Classical", example: "Am F# Db" },
  { id: "camelot", label: "Camelot", example: "8A 2B 3B" },
  { id: "openKey", label: "Open Key", example: "1m 7d 8d" },
];

export function DisplaySettings({ theme, notation }: { theme: Theme; notation: KeyNotation }) {
  const { update } = useSettings.getState();
  return (
    <>
      <SettingItem title="Theme" hint="Booth has the most contrast, for dark rooms.">
        <ChoiceTiles
          aria-label="Theme"
          columns={3}
          value={theme}
          onValueChange={(v) => void update({ theme: v as Theme })}
        >
          {THEMES.map((t) => (
            <ChoiceTile
              key={t.id}
              value={t.id}
              label={t.label}
              hint={t.hint}
              isDefault={t.id === "booth"}
              preview={<ThemePreview canvas={t.canvas} accent={t.accent} />}
            />
          ))}
        </ChoiceTiles>
      </SettingItem>
      <SettingItem title="Key notation" hint="How keys are shown. Key edits accept any notation.">
        <ChoiceTiles
          aria-label="Key notation"
          columns={3}
          value={notation}
          onValueChange={(v) => void update({ keyNotation: v as KeyNotation })}
        >
          {NOTATIONS.map((n) => (
            <ChoiceTile
              key={n.id}
              value={n.id}
              label={n.label}
              isDefault={n.id === "classical"}
              preview={<span className="font-mono text-sm text-fg">{n.example}</span>}
            />
          ))}
        </ChoiceTiles>
      </SettingItem>
    </>
  );
}
