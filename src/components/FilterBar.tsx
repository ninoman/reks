/**
 * Filters for the track list: a bar between the toolbar and the table (they
 * narrow the whole list), shown and hidden from the toolbar's Filters button.
 * A control that is filtering is outlined. The on/off switch lives in the
 * toolbar next to that button, so it works with the bar hidden. Options
 * (genres, keys, colours) come from `library_facets`. The playlist tree
 * chooses the playlist; Clear leaves that alone.
 */
import { type ReactNode, useEffect, useRef, useState } from "react";

import { activeFilterCount, type FilterState, useLibrary } from "../store/library";
import { useKeyLabel } from "../store/settings";

function triState(v: boolean | null): string {
  return v === null ? "" : String(v);
}

function parseTriState(s: string): boolean | null {
  return s === "" ? null : s === "true";
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
    <button
      type="button"
      role="switch"
      aria-checked={enabled}
      aria-label="Apply filters"
      title={enabled ? "Filters on: click to set them aside" : "Filters off: click to apply"}
      className={`filters-switch${enabled ? " on" : ""}`}
      onClick={() => setEnabled(!enabled)}
    />
  );
}

/** One labelled control in the bar; outlined while it filters. */
function Field({ label, set, children }: { label: string; set: boolean; children: ReactNode }) {
  return (
    <div className={`filter-field${set ? " set" : ""}`}>
      <span className="filter-label">{label}</span>
      {children}
    </div>
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
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

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
    <div className="key-picker" ref={ref}>
      <button
        type="button"
        className="filter-select"
        aria-label="Key"
        aria-haspopup="true"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {summary} ▾
      </button>
      {open && (
        <fieldset className="popover key-grid" aria-label="Keys">
          {keys?.map((k) => (
            <button
              key={k.key}
              type="button"
              className={chosen.includes(k.key) ? "on" : ""}
              aria-pressed={chosen.includes(k.key)}
              title={`${k.key} · ${k.camelot} · ${k.openKey}`}
              onClick={() => toggle(k.key)}
            >
              {label(k.key)}
            </button>
          ))}
        </fieldset>
      )}
    </div>
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
    <section className={`filter-bar${enabled ? "" : " off"}`} aria-label="Filters">
      <Field label="Genre" set={f.genre !== null}>
        <select
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
        </select>
      </Field>

      <Field label="Key" set={f.keys.length > 0}>
        <KeyPicker />
      </Field>

      <Field label="BPM" set={f.bpmMin !== null || f.bpmMax !== null}>
        <input
          type="number"
          className="bpm"
          aria-label="Minimum BPM"
          placeholder="min"
          min={0}
          value={f.bpmMin ?? ""}
          onChange={(e) => set({ bpmMin: parseBpm(e.target.value) })}
        />
        –
        <input
          type="number"
          className="bpm"
          aria-label="Maximum BPM"
          placeholder="max"
          min={0}
          value={f.bpmMax ?? ""}
          onChange={(e) => set({ bpmMax: parseBpm(e.target.value) })}
        />
      </Field>

      <Field label="Colour" set={f.color !== null}>
        <div className="colour-row">
          {facets?.colours.map((c) => (
            <button
              key={c}
              type="button"
              className={`swatch swatch-${c}${f.color === c ? " on" : ""}`}
              aria-pressed={f.color === c}
              aria-label={c}
              title={c}
              onClick={() => set({ color: f.color === c ? null : c })}
            />
          ))}
        </div>
      </Field>

      <Field label="Rating" set={f.ratingMin > 0}>
        <select
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
        </select>
      </Field>

      <Field label="File" set={f.missing !== null}>
        <select
          aria-label="File missing"
          value={triState(f.missing)}
          onChange={(e) => set({ missing: parseTriState(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="true">Missing</option>
          <option value="false">Present</option>
        </select>
      </Field>

      <Field label="Analysis" set={f.needsAnalysis !== null}>
        <select
          aria-label="Analysis"
          value={triState(f.needsAnalysis)}
          onChange={(e) => set({ needsAnalysis: parseTriState(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="true">Needs analysis</option>
          <option value="false">Analysed</option>
        </select>
      </Field>

      <button
        type="button"
        className="link clear-all"
        onClick={clearFilters}
        disabled={activeFilterCount(f) === 0}
      >
        Clear all
      </button>
    </section>
  );
}
