/** Track colour choice as a row of swatches; click the chosen one again to clear it. */
import { Swatch } from "./ui/Swatch";

export function ColourPicker({
  colours,
  value,
  onChange,
}: {
  colours: readonly string[];
  value: string | null;
  onChange: (colour: string | null) => void;
}) {
  return (
    <div className="flex gap-0.5">
      {colours.map((c) => (
        <Swatch key={c} color={c} size="md" asChild>
          <button
            type="button"
            className="cursor-pointer border-2 border-transparent p-0 aria-pressed:border-fg"
            aria-pressed={value === c}
            aria-label={c}
            title={c}
            onClick={() => onChange(value === c ? null : c)}
          />
        </Swatch>
      ))}
    </div>
  );
}
