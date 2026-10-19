/**
 * Filter panel on the right of the table. Two independent controls: the
 * switch applies the filters or sets them aside (values kept), and the
 * chevron collapses the panel to a narrow strip, which keeps the switch and
 * shows how many filters are set. Options (genres, keys, colours) come
 * from `library_facets`. The playlist tree chooses the playlist; Clear leaves
 * that alone.
 */
import { useState } from "react";

import { buildFilters, type FilterState, useLibrary } from "../store/library";

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

export function FiltersPanel() {
  const [open, setOpen] = useState(true);
  const facets = useLibrary((s) => s.facets);
  const f = useLibrary((s) => s.filters);
  const active = buildFilters({ ...f, playlistId: null }).length;
  const enabled = useLibrary((s) => s.filtersEnabled);
  const setEnabled = useLibrary((s) => s.setFiltersEnabled);
  const badge = active > 0 && (
    <span
      className={`filters-badge${enabled ? "" : " off"}`}
      title={enabled ? `${active} filters applied` : `${active} filters set, not applied`}
    >
      {active}
    </span>
  );
  const toggle = (
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
  const setFilters = useLibrary((s) => s.setFilters);
  const clearFilters = useLibrary((s) => s.clearFilters);
  const set = (patch: Partial<FilterState>) => setFilters(patch);

  const toggleKey = (key: string) =>
    set({ keys: f.keys.includes(key) ? f.keys.filter((k) => k !== key) : [...f.keys, key] });

  if (!open) {
    return (
      <aside className="filters-panel collapsed" aria-label="Filters">
        {toggle}
        <button
          type="button"
          className="filters-rail"
          aria-expanded={false}
          title="Show filters"
          onClick={() => setOpen(true)}
        >
          <span className="filters-rail-label">Filters</span>
          {badge}
        </button>
      </aside>
    );
  }

  return (
    <aside className={`filters-panel${enabled ? "" : " off"}`} aria-label="Filters">
      <div className="panel-head">
        <button
          type="button"
          className="panel-collapse"
          aria-expanded={true}
          title="Hide filters"
          onClick={() => setOpen(false)}
        >
          ▸
        </button>
        <h2>Filters</h2>
        {badge}
        <span className="spacer" />
        {toggle}
        <button type="button" onClick={clearFilters} disabled={active === 0}>
          Clear
        </button>
      </div>

      <label>
        Genre
        <select
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
      </label>

      <fieldset>
        <legend>Key</legend>
        <div className="key-grid">
          {facets?.keys.map((k) => (
            <button
              key={k.key}
              type="button"
              className={f.keys.includes(k.key) ? "on" : ""}
              aria-pressed={f.keys.includes(k.key)}
              title={k.key}
              onClick={() => toggleKey(k.key)}
            >
              {k.camelot}
            </button>
          ))}
        </div>
      </fieldset>

      <fieldset>
        <legend>BPM</legend>
        <div className="bpm-range">
          <input
            type="number"
            aria-label="Minimum BPM"
            placeholder="min"
            min={0}
            value={f.bpmMin ?? ""}
            onChange={(e) => set({ bpmMin: parseBpm(e.target.value) })}
          />
          –
          <input
            type="number"
            aria-label="Maximum BPM"
            placeholder="max"
            min={0}
            value={f.bpmMax ?? ""}
            onChange={(e) => set({ bpmMax: parseBpm(e.target.value) })}
          />
        </div>
      </fieldset>

      <fieldset>
        <legend>Colour</legend>
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
      </fieldset>

      <label>
        Rating at least
        <select value={f.ratingMin} onChange={(e) => set({ ratingMin: Number(e.target.value) })}>
          <option value={0}>Any</option>
          {[1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>
              {"★".repeat(n)}
            </option>
          ))}
        </select>
      </label>

      <label>
        File missing
        <select
          value={triState(f.missing)}
          onChange={(e) => set({ missing: parseTriState(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="true">Missing</option>
          <option value="false">Present</option>
        </select>
      </label>

      <label>
        Analysis
        <select
          value={triState(f.needsAnalysis)}
          onChange={(e) => set({ needsAnalysis: parseTriState(e.target.value) })}
        >
          <option value="">Any</option>
          <option value="true">Needs analysis</option>
          <option value="false">Analysed</option>
        </select>
      </label>
    </aside>
  );
}
