/**
 * Side panel for the primary selected track: artwork, editable metadata,
 * and read-only cue and beatgrid listings. Mount it with `key={trackId}` so
 * switching tracks starts from a fresh form (unsaved edits are dropped).
 */
import { type FormEvent, type KeyboardEvent, useCallback, useEffect, useState } from "react";

import {
  formatBpm,
  formatBpmRange,
  formatDuration,
  formatPosition,
  formatRgb,
  hotCueLabel,
} from "../lib/format";
import {
  artworkGet,
  type Cue,
  libraryTrack,
  libraryUpdateTrack,
  type TrackDetail,
} from "../lib/ipc";
import { useLibrary } from "../store/library";
import {
  buildPatch,
  type EditField,
  type EditForm,
  NUMBER_FIELDS,
  TEXT_FIELDS,
  toForm,
} from "./trackDetailForm";

const NO_COLOURS: string[] = [];

export function TrackDetailPanel({ trackId }: { trackId: number }) {
  const replaceRow = useLibrary((s) => s.replaceRow);
  const colours = useLibrary((s) => s.facets?.colours ?? NO_COLOURS);

  const [detail, setDetail] = useState<TrackDetail | null>(null);
  const [initial, setInitial] = useState<EditForm | null>(null);
  const [form, setForm] = useState<EditForm | null>(null);
  /** Number inputs holding something that isn't a number. */
  const [badInput, setBadInput] = useState<ReadonlySet<EditField>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const load = useCallback((d: TrackDetail) => {
    const f = toForm(d.track);
    setDetail(d);
    setInitial(f);
    setForm(f);
    setBadInput(new Set());
  }, []);

  useEffect(() => {
    let live = true;
    libraryTrack(trackId).then(
      (d) => live && load(d),
      (e: unknown) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [trackId, load]);

  if (detail === null || initial === null || form === null) {
    return (
      <aside className="detail-panel" aria-label="Track details">
        {error !== null ? (
          <div className="error" role="alert">
            {error}
          </div>
        ) : (
          <p className="muted">Loading…</p>
        )}
      </aside>
    );
  }

  const patch = buildPatch(initial, form);
  const dirty = Object.keys(patch).length > 0;
  const t = detail.track;

  function set(field: EditField, value: string, bad = false) {
    setForm((f) => (f === null ? f : { ...f, [field]: value }));
    setBadInput((b) => {
      const next = new Set(b);
      if (bad) next.add(field);
      else next.delete(field);
      return next;
    });
  }

  function revert() {
    setForm(initial);
    setBadInput(new Set());
    setError(null);
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    if (!dirty || badInput.size > 0) return;
    setSaving(true);
    try {
      const d = await libraryUpdateTrack(trackId, patch);
      load(d);
      setError(null);
      replaceRow(d.track);
      void useLibrary.getState().loadFacets();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      revert();
    }
  }

  return (
    <aside className="detail-panel" aria-label="Track details">
      <ArtworkView id={t.artwork?.id ?? null} mime={t.artwork?.mime ?? ""} />
      <div className="detail-facts">
        <div>
          {t.kind.toUpperCase()}
          {t.bitRateKbps !== null && ` · ${t.bitRateKbps} kbps`}
          {t.sampleRate !== null && ` · ${(t.sampleRate / 1000).toFixed(1)} kHz`}
          {t.durationMs !== null && ` · ${formatDuration(t.durationMs)}`}
          {t.bpmX100 !== null && ` · ${formatBpm(t.bpmX100)} BPM`}
        </div>
        <div className="path" title={t.path}>
          {t.missing ? "Missing: " : ""}
          {t.path}
        </div>
      </div>

      <form className="detail-form" onSubmit={(e) => void save(e)} onKeyDown={onKeyDown}>
        {TEXT_FIELDS.map(({ id, label, multiline }) =>
          multiline ? (
            <label key={id}>
              {label}
              <textarea rows={3} value={form[id]} onChange={(e) => set(id, e.target.value)} />
            </label>
          ) : (
            <label key={id}>
              {label}
              <input type="text" value={form[id]} onChange={(e) => set(id, e.target.value)} />
            </label>
          ),
        )}
        <div className="detail-row">
          {NUMBER_FIELDS.map(({ id, label }) => (
            <label key={id}>
              {label}
              <input
                type="number"
                step={1}
                value={form[id]}
                onChange={(e) => set(id, e.target.value, e.target.validity.badInput)}
              />
            </label>
          ))}
        </div>
        <div className="detail-row">
          <label>
            Rating
            <select value={form.rating} onChange={(e) => set("rating", e.target.value)}>
              {[0, 1, 2, 3, 4, 5].map((r) => (
                <option key={r} value={String(r)}>
                  {r === 0 ? "None" : "★".repeat(r)}
                </option>
              ))}
            </select>
          </label>
          <label>
            Colour
            <select value={form.color} onChange={(e) => set("color", e.target.value)}>
              <option value="">None</option>
              {/* Keep a stored colour selectable even before facets load. */}
              {(colours.includes(form.color) || form.color === ""
                ? colours
                : [form.color, ...colours]
              ).map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
          </label>
          <label>
            Key
            <input
              type="text"
              value={form.key}
              placeholder="Am, 8A, 1m"
              onChange={(e) => set("key", e.target.value)}
            />
          </label>
        </div>
        {error !== null && (
          <div className="error" role="alert">
            {error}
          </div>
        )}
        {badInput.size > 0 && <div className="error">Numbers only in Track #, Disc #, Year.</div>}
        <div className="detail-actions">
          <button type="button" onClick={revert} disabled={!dirty || saving}>
            Revert
          </button>
          <button type="submit" disabled={!dirty || saving || badInput.size > 0}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>

      <section className="detail-section">
        <h3>Beatgrid</h3>
        {detail.beatgrid === null ? (
          <p className="muted">No beatgrid (needs analysis)</p>
        ) : (
          <dl>
            <dt>BPM</dt>
            <dd>{formatBpmRange(detail.beatgrid.bpmMinX100, detail.beatgrid.bpmMaxX100)}</dd>
            <dt>First beat</dt>
            <dd>{formatPosition(detail.beatgrid.firstBeatMs)}</dd>
            <dt>Markers</dt>
            <dd>{detail.beatgrid.markers}</dd>
          </dl>
        )}
      </section>

      <section className="detail-section">
        <h3>Cues</h3>
        {detail.cues.length === 0 ? (
          <p className="muted">No cues</p>
        ) : (
          <table className="cue-list">
            <tbody>
              {detail.cues.map((c) => (
                <CueRow key={c.id} cue={c} />
              ))}
            </tbody>
          </table>
        )}
      </section>
    </aside>
  );
}

function CueRow({ cue }: { cue: Cue }) {
  const what = cue.slot === null ? "Memory" : `Hot ${hotCueLabel(cue.slot)}`;
  return (
    <tr>
      <td>
        {cue.colorRgb !== null && (
          <span className="cue-swatch" style={{ background: formatRgb(cue.colorRgb) }} />
        )}
        {what}
        {cue.endMs !== null && " loop"}
      </td>
      <td className="pos">
        {formatPosition(cue.startMs)}
        {cue.endMs !== null && ` – ${formatPosition(cue.endMs)}`}
      </td>
      <td>{cue.name ?? ""}</td>
    </tr>
  );
}

/** Artwork from `artwork_get` bytes; a placeholder when there is none or it fails. */
function ArtworkView({ id, mime }: { id: number | null; mime: string }) {
  const [loaded, setLoaded] = useState<{ id: number; url: string } | null>(null);

  useEffect(() => {
    if (id === null) return;
    let live = true;
    let created: string | null = null;
    artworkGet(id).then(
      (bytes) => {
        if (!live) return;
        created = URL.createObjectURL(new Blob([bytes], { type: mime }));
        setLoaded({ id, url: created });
      },
      () => {
        // A deleted cache file: show the placeholder.
      },
    );
    return () => {
      live = false;
      if (created !== null) URL.revokeObjectURL(created);
    };
  }, [id, mime]);

  // A stale picture from the previous id is not shown while the new one loads.
  const url = loaded !== null && loaded.id === id ? loaded.url : null;
  return url === null ? (
    <div className="artwork placeholder" role="img" aria-label="No artwork">
      ♪
    </div>
  ) : (
    <img className="artwork" src={url} alt="Artwork" />
  );
}
