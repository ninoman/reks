/**
 * Filters for the track list: a bar between the toolbar and the table (they
 * narrow the whole list), shown and hidden from the toolbar's Filters button.
 * A control that is filtering is outlined. The on/off switch lives in the
 * toolbar next to that button, so it works with the bar hidden. Options
 * (genres, keys, colours) come from `library_facets`. The playlist tree
 * chooses the playlist; Clear leaves that alone.
 */
import { type ReactNode, useState } from "react";

import { cn } from "../lib/cn";
import {
  activeFilterCount,
  type FilterState,
  type LastPlayedFilter,
  useLibrary,
} from "../store/library";
import { useKeyLabel } from "../store/settings";
import { ChevronDownIcon } from "./icons";
import { Button } from "./ui/Button";
import { Field as UiField } from "./ui/Field";
import { Input } from "./ui/Input";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/Popover";
import { Select } from "./ui/Select";
import { Swatch } from "./ui/Swatch";
import { Switch } from "./ui/Switch";
import { Tooltip } from "./ui/Tooltip";

function triState(v: boolean | null): string {
  return v === null ? "" : String(v);
}

function parseTriState(s: string): boolean | null {
  return s === "" ? null : s === "true";
}

function parseCount(s: string): number | null {
  const n = Number.parseInt(s, 10);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

function parseLastPlayed(s: string): LastPlayedFilter | null {
  return s === "" ? null : s === "never" ? "never" : (Number(s) as LastPlayedFilter);
}

function parseBpm(s: string): number | null {
  const n = Number.parseFloat(s);
  return Number.isFinite(n) && n > 0 ? n : null;
}

/** The on/off switch: applies the filters or sets them aside (values kept). */
export function FiltersSwitch() {
  const enabled = useLibrary((s) => s.filtersEnabled);
  const setEnabled = useLibrary((s) => s.setFiltersEnabled);
  return (
    <Tooltip
      content={enabled ? "Filters on: click to set them aside" : "Filters off: click to apply"}
    >
      <Switch aria-label="Apply filters" checked={enabled} onCheckedChange={setEnabled} />
    </Tooltip>
  );
}

/** One labelled control in the bar; outlined while it filters. */
function Field({ label, set, children }: { label: string; set: boolean; children: ReactNode }) {
  return (
    <UiField label={label} orientation="inline" active={set}>
      {children}
    </UiField>
  );
}

/**
 * Key picker: a button showing the chosen keys and a popover laid out as the
 * Camelot wheel, labelled in the notation chosen in Settings.
 */
function KeyPicker() {
  const keys = useLibrary((s) => s.facets?.keys);
  const chosen = useLibrary((s) => s.filters.keys);
  const setFilters = useLibrary((s) => s.setFilters);
  const [open, setOpen] = useState(false);

  const label = useKeyLabel();
  const summary =
    chosen.length === 0
      ? "Any"
      : chosen.length <= 2
        ? chosen.map(label).join(", ")
        : `${chosen.length} keys`;
  const toggle = (key: string) =>
    setFilters({
      keys: chosen.includes(key) ? chosen.filter((k) => k !== key) : [...chosen, key],
    });

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" className="min-w-16 justify-between font-normal" aria-label="Key">
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
              aria-pressed={chosen.includes(k.key)}
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

export function FilterBar() {
  const facets = useLibrary((s) => s.facets);
  const f = useLibrary((s) => s.filters);
  const enabled = useLibrary((s) => s.filtersEnabled);
  const setFilters = useLibrary((s) => s.setFilters);
  const clearFilters = useLibrary((s) => s.clearFilters);
  const set = (patch: Partial<FilterState>) => setFilters(patch);

  return (
    <section
      className={cn(
        "flex shrink-0 flex-wrap items-center gap-x-2.5 gap-y-1.5 border-b border-border bg-bg-subtle px-2.5 py-1.5",
        !enabled && "[&_[data-slot=field]]:opacity-45",
      )}
      aria-label="Filters"
    >
      <Field label="Genre" set={f.genre !== null}>
        <Select
          size="sm"
          aria-label="Genre"
          value={f.genre ?? ""}
          onChange={(e) => set({ genre: e.target.value === "" ? null : e.target.value })}
        >
          <option value="">Any</option>
          {facets?.genres.map((g) => (
            <option key={g} value={g}>
              {g}
            </option>
          ))}
        </Select>
      </Field>

      <Field label="Key" set={f.keys.length > 0}>
        <KeyPicker />
      </Field>

      <Field label="BPM" set={f.bpmMin !== null || f.bpmMax !== null}>
        <Input
          size="sm"
          type="number"
          className="w-14"
          aria-label="Minimum BPM"
          placeholder="min"
          min={0}
          value={f.bpmMin ?? ""}
          onChange={(e) => set({ bpmMin: parseBpm(e.target.value) })}
        />
        –
        <Input
          size="sm"
          type="number"
          className="w-14"
          aria-label="Maximum BPM"
          placeholder="max"
          min={0}
          value={f.bpmMax ?? ""}
          onChange={(e) => set({ bpmMax: parseBpm(e.target.value) })}
        />
      </Field>

      <Field label="Colour" set={f.color !== null}>
        <div className="flex gap-0.5">
          {facets?.colours.map((c) => (
            <Swatch key={c} color={c} size="md" asChild>
              <button
                type="button"
                className="cursor-pointer border-2 border-transparent p-0 aria-pressed:border-fg"
                aria-pressed={f.color === c}
                aria-label={c}
                title={c}
                onClick={() => set({ color: f.color === c ? null : c })}
              />
            </Swatch>
          ))}
        </div>
      </Field>

      <Field label="Rating" set={f.ratingMin > 0}>
        <Select
          size="sm"
          aria-label="Rating at least"
          value={f.ratingMin}
          onChange={(e) => set({ ratingMin: Number(e.target.value) })}
        >
          <option value={0}>Any</option>
          {[1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>
              {"★".repeat(n)}+
            </option>
          ))}
        </Select>
      </Field>

      <Field label="File" set={f.missing !== null}>
        <Select
          size="sm"
          aria-label="File missing"
          value={triState(f.missing)}
          onChange={(e) => set({ missing: parseTriState(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="true">Missing</option>
          <option value="false">Present</option>
        </Select>
      </Field>

      <Field label="Analysis" set={f.needsAnalysis !== null}>
        <Select
          size="sm"
          aria-label="Analysis"
          value={triState(f.needsAnalysis)}
          onChange={(e) => set({ needsAnalysis: parseTriState(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="true">Needs analysis</option>
          <option value="false">Analysed</option>
        </Select>
      </Field>

      <Field label="Plays" set={f.playsMin !== null || f.playsMax !== null}>
        <Input
          size="sm"
          type="number"
          className="w-14"
          aria-label="Minimum plays"
          placeholder="min"
          min={0}
          value={f.playsMin ?? ""}
          onChange={(e) => set({ playsMin: parseCount(e.target.value) })}
        />
        –
        <Input
          size="sm"
          type="number"
          className="w-14"
          aria-label="Maximum plays"
          placeholder="max"
          min={0}
          value={f.playsMax ?? ""}
          onChange={(e) => set({ playsMax: parseCount(e.target.value) })}
        />
      </Field>

      <Field label="Last played" set={f.lastPlayed !== null}>
        <Select
          size="sm"
          aria-label="Last played"
          value={f.lastPlayed ?? ""}
          onChange={(e) => set({ lastPlayed: parseLastPlayed(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="never">Never played</option>
          <option value="30">Last 30 days</option>
          <option value="90">Last 90 days</option>
          <option value="365">Last year</option>
        </Select>
      </Field>

      <Button
        size="sm"
        variant="link"
        className="ml-auto"
        onClick={clearFilters}
        disabled={activeFilterCount(f) === 0}
      >
        Clear all
      </Button>
    </section>
  );
}
