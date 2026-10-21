/**
 * Key picker: a button showing the chosen keys and a popover laid out as the
 * Camelot wheel, labelled in the notation chosen in Settings. Used by the
 * filter bar and the rule builder's Key rows.
 */
import { useState } from "react";

import type { Key } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { useKeyLabel } from "../store/settings";
import { ChevronDownIcon } from "./icons";
import { Button } from "./ui/Button";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/Popover";

export function KeyPicker({
  value,
  onChange,
  label: ariaLabel = "Key",
  emptyLabel = "Any",
}: {
  value: readonly Key[];
  onChange: (keys: Key[]) => void;
  label?: string;
  /** Shown on the button while no key is ticked. */
  emptyLabel?: string;
}) {
  const keys = useLibrary((s) => s.facets?.keys);
  const [open, setOpen] = useState(false);

  const label = useKeyLabel();
  const summary =
    value.length === 0
      ? emptyLabel
      : value.length <= 2
        ? value.map(label).join(", ")
        : `${value.length} keys`;
  const toggle = (key: string) =>
    onChange(value.includes(key) ? value.filter((k) => k !== key) : [...value, key]);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" className="min-w-16 justify-between font-normal" aria-label={ariaLabel}>
          {summary}
          <ChevronDownIcon />
        </Button>
      </PopoverTrigger>
      <PopoverContent>
        <fieldset
          className="m-0 grid grid-cols-[repeat(6,36px)] gap-0.5 border-0 p-0"
          aria-label="Keys"
        >
          {keys?.map((k) => (
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
      </PopoverContent>
    </Popover>
  );
}
