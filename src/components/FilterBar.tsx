/**
 * Filters for the track list: a bar between the toolbar and the table (they
 * narrow the whole list), shown and hidden from the toolbar's Filters button.
 * A control that is filtering is outlined. The on/off switch lives in the
 * toolbar next to that button, so it works with the bar hidden. Options
 * (genres, keys, colours) come from `library_facets`. The playlist tree
 * chooses the playlist; Clear leaves that alone.
 */
import type { ReactNode } from "react";

import { cn } from "../lib/cn";
import {
  activeFilterCount,
  type FilterState,
  type LastPlayedFilter,
  useLibrary,
} from "../store/library";
import { clearAllFilters, useRules } from "../store/rules";
import { ruleLeafCount } from "../store/ruleTree";
import { ColourPicker } from "./ColourPicker";
import { KeyPicker } from "./KeyPicker";
import { TagFilterPicker } from "./TagFilterPicker";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Field as UiField } from "./ui/Field";
import { Input } from "./ui/Input";
import { Select } from "./ui/Select";
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

export function FilterBar() {
  const facets = useLibrary((s) => s.facets);
  const f = useLibrary((s) => s.filters);
  const enabled = useLibrary((s) => s.filtersEnabled);
  const setFilters = useLibrary((s) => s.setFilters);
  const ruleCount = useLibrary((s) => ruleLeafCount(s.rule));
  const rulesOpen = useRules((s) => s.open);
  const toggleRules = useRules((s) => s.toggleOpen);
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
        <KeyPicker value={f.keys} onChange={(keys) => set({ keys })} />
      </Field>

      <Field label="My Tags" set={f.tagIds.length > 0}>
        <TagFilterPicker ids={f.tagIds} onChange={(tagIds) => set({ tagIds })} />
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
        <ColourPicker
          colours={facets?.colours ?? []}
          value={f.color}
          onChange={(color) => set({ color })}
        />
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
        variant="outline"
        className="ml-auto"
        aria-expanded={rulesOpen}
        aria-controls={rulesOpen ? "rule-builder" : undefined}
        onClick={toggleRules}
      >
        Advanced
        {ruleCount > 0 && (
          <>
            <Badge aria-hidden variant={enabled ? "accent" : "muted"}>
              {ruleCount}
            </Badge>
            <span className="sr-only">, {ruleCount} active</span>
          </>
        )}
      </Button>
      <Button
        size="sm"
        variant="link"
        onClick={clearAllFilters}
        disabled={activeFilterCount(f) + ruleCount === 0}
      >
        Clear all
      </Button>
    </section>
  );
}
