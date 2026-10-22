/**
 * Track info panel (right of the table) for the primary selected track:
 * artwork, editable metadata, "Played in" (play history), the read-only cue
 * listing, and the beatgrid summary with its editor (`GridEditor.tsx`).
 * `TrackDetailPanel` is mounted with `key={trackId}` so switching tracks
 * starts from a fresh form (unsaved edits are dropped).
 */
import {
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

import {
  formatBpm,
  formatBpmRange,
  formatDuration,
  formatPosition,
  formatRgb,
  hotCueLabel,
} from "../lib/format";
import { type Cue, libraryTrack, libraryUpdateTrack, type TrackDetail } from "../lib/ipc";
import { rowAt, useLibrary } from "../store/library";
import { usePanels } from "../store/panels";
import { replaceRowEverywhere } from "../store/panes";
import { primaryIndex } from "../store/selection";
import { useKeyLabel } from "../store/settings";
import { ArtworkThumb } from "./ArtworkThumb";
import { GridEditorDialog } from "./GridEditor";
import { PlayedIn } from "./PlayedIn";
import { TrackTags } from "./TrackTags";
import {
  buildPatch,
  type EditField,
  type EditForm,
  NUMBER_FIELDS,
  TEXT_FIELDS,
  toForm,
} from "./trackDetailForm";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";
import { Input } from "./ui/Input";
import { ScrollArea } from "./ui/ScrollArea";
import { Select } from "./ui/Select";
import { SheetHeader } from "./ui/Sheet";
import { Swatch } from "./ui/Swatch";
import { Textarea } from "./ui/Textarea";

/** The panel: a fixed head, then everything else scrolls. */
function Panel({ children }: { children: ReactNode }) {
  const toggleInfo = usePanels((s) => s.toggleInfo);
  return (
    <aside className="flex min-h-0 flex-1 flex-col" aria-label="Track details">
      <div className="px-3 pt-2">
        <SheetHeader title="Track info" closeLabel="Close track info" onClose={toggleInfo} />
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-2.5 px-3 pt-1 pb-3">{children}</div>
      </ScrollArea>
    </aside>
  );
}

const muted = "m-0 text-fg-muted";
const sectionTitle = "mt-0 mb-1 text-sm font-semibold";
const facts = "m-0 grid grid-cols-[auto_1fr] gap-x-2.5 gap-y-0.5 [&_dt]:text-fg-muted [&_dd]:m-0";

const NO_COLOURS: string[] = [];

/** The info panel for the primary selected track, or a hint when none is. */
export function TrackInfoPanel() {
  const primaryId = useLibrary((s) => {
    const i = primaryIndex(s.selection);
    return i === null ? null : (rowAt(s, i)?.id ?? null);
  });
  if (primaryId === null) {
    return (
      <Panel>
        <p className={muted}>Select a track to see and edit its details.</p>
      </Panel>
    );
  }
  return <TrackDetailPanel key={primaryId} trackId={primaryId} />;
}

export function TrackDetailPanel({ trackId }: { trackId: number }) {
  const colours = useLibrary((s) => s.facets?.colours ?? NO_COLOURS);

  const [detail, setDetail] = useState<TrackDetail | null>(null);
  const [initial, setInitial] = useState<EditForm | null>(null);
  const [form, setForm] = useState<EditForm | null>(null);
  /** Number inputs holding something that isn't a number. */
  const [badInput, setBadInput] = useState<ReadonlySet<EditField>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [editingGrid, setEditingGrid] = useState(false);

  // Latest label without reloading the form when the notation changes.
  const label = useKeyLabel();
  const keyLabel = useRef(label);
  keyLabel.current = label;

  const load = useCallback((d: TrackDetail) => {
    const f = toForm(d.track, keyLabel.current);
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

  // A grid edit changes the summary and the track's BPM, not the form.
  const gridChanged = useCallback(() => {
    libraryTrack(trackId).then(
      (d) => {
        setDetail((cur) => (cur === null ? d : { ...cur, beatgrid: d.beatgrid, track: d.track }));
        replaceRowEverywhere(d.track);
      },
      (e: unknown) => setError(String(e)),
    );
  }, [trackId]);

  if (detail === null || initial === null || form === null) {
    return (
      <Panel>
        {error !== null ? (
          <div className="text-danger select-text" role="alert">
            {error}
          </div>
        ) : (
          <p className={muted}>Loading…</p>
        )}
      </Panel>
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
      replaceRowEverywhere(d.track);
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
    <Panel>
      <ArtworkThumb artworkId={t.artwork?.id ?? null} mime={t.artwork?.mime ?? ""} />
      <div className="text-xs text-fg-muted">
        <div>
          {t.kind.toUpperCase()}
          {t.bitRateKbps !== null && ` · ${t.bitRateKbps} kbps`}
          {t.sampleRate !== null && ` · ${(t.sampleRate / 1000).toFixed(1)} kHz`}
          {t.durationMs !== null && ` · ${formatDuration(t.durationMs)}`}
          {t.bpmX100 !== null && ` · ${formatBpm(t.bpmX100)} BPM`}
        </div>
        <div className="truncate font-mono select-text" title={t.path}>
          {t.missing ? "Missing: " : ""}
          {t.path}
        </div>
      </div>

      <form className="flex flex-col gap-1.5" onSubmit={(e) => void save(e)} onKeyDown={onKeyDown}>
        {TEXT_FIELDS.map(({ id, label, multiline }) =>
          multiline ? (
            <Field key={id} label={label}>
              <Textarea rows={3} value={form[id]} onChange={(e) => set(id, e.target.value)} />
            </Field>
          ) : (
            <Field key={id} label={label}>
              <Input type="text" value={form[id]} onChange={(e) => set(id, e.target.value)} />
            </Field>
          ),
        )}
        <div className="flex gap-1.5 [&>*]:flex-1">
          {NUMBER_FIELDS.map(({ id, label }) => (
            <Field key={id} label={label}>
              <Input
                type="number"
                step={1}
                value={form[id]}
                onChange={(e) => set(id, e.target.value, e.target.validity.badInput)}
              />
            </Field>
          ))}
        </div>
        <div className="flex gap-1.5 [&>*]:flex-1">
          <Field label="Rating">
            <Select value={form.rating} onChange={(e) => set("rating", e.target.value)}>
              {[0, 1, 2, 3, 4, 5].map((r) => (
                <option key={r} value={String(r)}>
                  {r === 0 ? "None" : "★".repeat(r)}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Colour">
            <Select value={form.color} onChange={(e) => set("color", e.target.value)}>
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
            </Select>
          </Field>
          <Field label="Key">
            <Input
              type="text"
              value={form.key}
              placeholder="Am, 8A, 1m"
              onChange={(e) => set("key", e.target.value)}
            />
          </Field>
        </div>
        {error !== null && (
          <div className="text-danger select-text" role="alert">
            {error}
          </div>
        )}
        {badInput.size > 0 && (
          <div className="text-danger">Numbers only in Track #, Disc #, Year.</div>
        )}
        <div className="mt-1 flex justify-end gap-1.5">
          <Button onClick={revert} disabled={!dirty || saving}>
            Revert
          </Button>
          <Button type="submit" variant="primary" disabled={!dirty || saving || badInput.size > 0}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </form>

      <TrackTags trackId={trackId} fallback={t.tags} />

      <PlayedIn trackId={trackId} playCount={t.playCount} lastPlayed={t.lastPlayed} />

      <section>
        <div className="mb-1 flex items-center justify-between">
          <h3 className={`${sectionTitle} mb-0`}>Beatgrid</h3>
          <Button size="xs" variant="outline" onClick={() => setEditingGrid(true)}>
            Edit grid
          </Button>
        </div>
        {editingGrid && (
          <GridEditorDialog
            trackId={trackId}
            title={t.title ?? "Untitled"}
            onClose={() => setEditingGrid(false)}
            onChanged={gridChanged}
          />
        )}
        {detail.beatgrid === null ? (
          <p className={muted}>No beatgrid (needs analysis)</p>
        ) : (
          <dl className={facts}>
            <dt>BPM</dt>
            <dd>{formatBpmRange(detail.beatgrid.bpmMinX100, detail.beatgrid.bpmMaxX100)}</dd>
            <dt>First beat</dt>
            <dd>{formatPosition(detail.beatgrid.firstBeatMs)}</dd>
            <dt>Markers</dt>
            <dd>{detail.beatgrid.markers}</dd>
          </dl>
        )}
      </section>

      <section>
        <h3 className={sectionTitle}>Cues</h3>
        {detail.cues.length === 0 ? (
          <p className={muted}>No cues</p>
        ) : (
          <table
            className="w-full border-collapse [&_td]:py-0.5 [&_td]:pr-1 [&_td]:align-top"
            aria-label="Cues"
          >
            <tbody>
              {detail.cues.map((c) => (
                <CueRow key={c.id} cue={c} />
              ))}
            </tbody>
          </table>
        )}
      </section>
    </Panel>
  );
}

function CueRow({ cue }: { cue: Cue }) {
  const what = cue.slot === null ? "Memory" : `Hot ${hotCueLabel(cue.slot)}`;
  return (
    <tr>
      <td>
        {cue.colorRgb !== null && (
          <Swatch size="xs" className="mr-1" style={{ background: formatRgb(cue.colorRgb) }} />
        )}
        {what}
        {cue.endMs !== null && " loop"}
      </td>
      <td className="whitespace-nowrap tabular-nums" data-slot="cue-pos">
        {formatPosition(cue.startMs)}
        {cue.endMs !== null && ` – ${formatPosition(cue.endMs)}`}
      </td>
      <td>{cue.name ?? ""}</td>
    </tr>
  );
}
