/**
 * The Key filter's choices, laid out as the Camelot wheel and labelled in
 * the notation chosen in Settings (the caller passes `label`). A picker
 * body: the Filters drawer's Key row puts it in a `PickerPopover`.
 */
import type { Key, KeyInfo } from "../lib/ipc";
import { Button } from "./ui/Button";

export function KeyGrid({
  keys,
  value,
  onChange,
  label,
}: {
  keys: readonly KeyInfo[];
  value: readonly Key[];
  onChange: (keys: Key[]) => void;
  label: (key: Key) => string;
}) {
  const toggle = (key: Key) =>
    onChange(value.includes(key) ? value.filter((k) => k !== key) : [...value, key]);
  return (
    <fieldset
      className="m-0 grid grid-cols-[repeat(6,36px)] gap-0.5 border-0 p-0"
      aria-label="Keys"
    >
      {keys.map((k) => (
        <Button
          key={k.key}
          size="xs"
          variant="ghost"
          className="px-0 font-normal"
          aria-pressed={value.includes(k.key)}
          title={`${k.key} · ${k.camelot} · ${k.openKey}`}
          onClick={() => toggle(k.key)}
        >
          {label(k.key)}
        </Button>
      ))}
    </fieldset>
  );
}
