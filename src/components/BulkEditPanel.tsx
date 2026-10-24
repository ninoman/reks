/**
 * The info panel while 2+ tracks are selected: one form for the fields that
 * are safe to set on many tracks at once (not title, BPM, grid, cues, path).
 * A field where the tracks differ shows "Mixed" and is left alone unless the
 * user changes it; My Tags are added to or removed from all. Apply sends one
 * `library_bulk_update_tracks` call (a single transaction in `reks-core`).
 * Mounted with `key` = the selected ids, so another selection starts clean.
 */
import { type FormEvent, type KeyboardEvent, useState } from "react";

import { libraryBulkUpdateTracks, type TrackRow } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { replaceRowEverywhere } from "../store/panes";
import { useSelectedTracks } from "../store/selectedTracks";
import { useKeyLabel } from "../store/settings";
import { coverage } from "../store/tagModel";
import { useTags } from "../store/tags";
import { useToasts } from "../store/toasts";
import {
  BULK_CONFIRM_FROM,
  BULK_TEXT_FIELDS,
  type BulkField,
  type BulkForm,
  buildBulkPatch,
  type CommentMode,
  changeSummary,
  countChanged,
  emptyBulkForm,
  setCommentMode,
  setCommentText,
  setField,
  sharedValue,
  toggleTag,
} from "./bulkEditForm";
import { ConfirmDialog } from "./ConfirmDialog";
import { ErrorMessage } from "./ErrorMessage";
import { InfoPanel, infoMuted } from "./InfoPanel";
import { TagPicker } from "./TagPicker";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";
import { Input } from "./ui/Input";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { Select } from "./ui/Select";
import { Textarea } from "./ui/Textarea";

const MIXED = "__mixed";
const NO_COLOURS: string[] = [];

export function BulkEditForm({ rows }: { rows: TrackRow[] }) {
  const colours = useLibrary((s) => s.facets?.colours ?? NO_COLOURS);
  const keyLabel = useKeyLabel();
  const [form, setForm] = useState<BulkForm>(emptyBulkForm);
  const [badInput, setBadInput] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const n = rows.length;
  const changed = countChanged(rows, form, keyLabel);
  const touched =
    Object.keys(form.fields).length > 0 || form.comment !== null || form.tags.size > 0;
  const canApply = changed > 0 && !saving && !badInput;

  /** What an untouched or edited field shows: its value, or "Mixed" for differing tracks. */
  function shown(field: BulkField) {
    const edit = form.fields[field];
    if (edit !== undefined) return { value: edit, mixed: false };
    const s = sharedValue(rows, field);
    if (s.kind === "mixed") return { value: "", mixed: true };
    return { value: field === "key" && s.value !== "" ? keyLabel(s.value) : s.value, mixed: false };
  }

  function set(field: BulkField, value: string) {
    setForm((f) => setField(rows, f, field, value));
  }

  function discard() {
    setForm(emptyBulkForm);
    setBadInput(false);
    setError(null);
  }

  async function apply() {
    setConfirming(false);
    setSaving(true);
    try {
      const out = await libraryBulkUpdateTracks(
        rows.map((r) => r.id),
        buildBulkPatch(form),
      );
      for (const row of out.rows) replaceRowEverywhere(row);
      void useLibrary.getState().loadFacets();
      void useTags.getState().load();
      useToasts.getState().show(`Updated ${out.rows.length} tracks`);
      discard();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    if (!canApply) return;
    if (changed >= BULK_CONFIRM_FROM) setConfirming(true);
    else void apply();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape" && !confirming) {
      e.preventDefault();
      discard();
    }
  }

  const textInput = (id: BulkField, label: string) => {
    const s = shown(id);
    return (
      <Field key={id} label={label}>
        <Input
          type="text"
          value={s.value}
          placeholder={s.mixed ? "Mixed" : undefined}
          onChange={(e) => set(id, e.target.value)}
        />
      </Field>
    );
  };

  const year = shown("year");
  const rating = shown("rating");
  const color = shown("color");
  const key = shown("key");
  const mode = form.commentMode;
  const commentShared = sharedValue(rows, "comment");
  const commentText =
    form.comment !== null
      ? form.comment.text
      : mode === "replace" && commentShared.kind === "same"
        ? commentShared.value
        : "";
  const commentMixed =
    form.comment === null && mode === "replace" && commentShared.kind === "mixed";
  const tagCounts = (tag: number) => rows.filter((r) => r.tags.includes(tag)).length;

  return (
    <form className="flex flex-col gap-1.5" onSubmit={submit} onKeyDown={onKeyDown}>
      <p className="m-0 text-fg-muted">
        <strong className="text-fg">{n} tracks selected.</strong> Fields that show “Mixed” differ
        between them and stay as they are unless you change them.
      </p>
      {BULK_TEXT_FIELDS.map((f) => textInput(f.id, f.label))}
      <div className="flex gap-1.5 [&>*]:flex-1">
        <Field label="Year">
          <Input
            type="number"
            step={1}
            value={year.value}
            placeholder={year.mixed ? "Mixed" : undefined}
            onChange={(e) => {
              setBadInput(e.target.validity.badInput);
              set("year", e.target.value);
            }}
          />
        </Field>
        <Field label="Rating">
          <Select
            value={rating.mixed ? MIXED : rating.value}
            onChange={(e) => set("rating", e.target.value)}
          >
            {rating.mixed && (
              <option value={MIXED} disabled hidden>
                Mixed
              </option>
            )}
            {[0, 1, 2, 3, 4, 5].map((r) => (
              <option key={r} value={String(r)}>
                {r === 0 ? "None" : "★".repeat(r)}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className="flex gap-1.5 [&>*]:flex-1">
        <Field label="Colour">
          <Select
            value={color.mixed ? MIXED : color.value}
            onChange={(e) => set("color", e.target.value)}
          >
            {color.mixed && (
              <option value={MIXED} disabled hidden>
                Mixed
              </option>
            )}
            <option value="">None</option>
            {colours.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Key">
          <Input
            type="text"
            value={key.value}
            placeholder={key.mixed ? "Mixed" : "Am, 8A, 1m"}
            onChange={(e) => set("key", e.target.value)}
          />
        </Field>
      </div>

      <SegmentedControl
        size="sm"
        aria-label="Comment edit"
        value={mode}
        onValueChange={(v) => setForm((f) => setCommentMode(f, v as CommentMode))}
      >
        <Segment value="replace">Replace comment</Segment>
        <Segment value="append">Append to comment</Segment>
      </SegmentedControl>
      <Field label="Comment">
        <Textarea
          rows={3}
          value={commentText}
          placeholder={
            commentMixed ? "Mixed" : mode === "append" ? "Added after each comment" : undefined
          }
          onChange={(e) => setForm((f) => setCommentText(f, e.target.value))}
        />
      </Field>

      <section aria-label="My Tags">
        <h3 className="mt-1 mb-1 text-sm font-semibold">My Tags</h3>
        <p className="mt-0 mb-1 text-xs text-fg-muted">
          Ticking adds a tag to every selected track, unticking removes it from all. A dash means
          some have it.
        </p>
        <TagPicker
          coverageOf={(tag) => {
            const intent = form.tags.get(tag);
            if (intent !== undefined) return intent === "add" ? "all" : "none";
            return coverage(
              rows.map((r) => r.tags),
              tag,
            );
          }}
          onToggle={(tag) => setForm((f) => toggleTag(f, tag, n, tagCounts(tag)))}
        />
      </section>

      {error !== null && <ErrorMessage message={error} />}
      {badInput && <div className="text-danger">Numbers only in Year.</div>}
      <div className="mt-1 flex justify-end gap-1.5">
        <Button onClick={discard} disabled={!touched || saving}>
          Discard
        </Button>
        <Button type="submit" variant="primary" disabled={!canApply}>
          {saving ? "Saving…" : changed === 0 ? "Apply" : `Apply to ${changed} tracks`}
        </Button>
      </div>
      {confirming && (
        <ConfirmDialog
          title={`Change ${changed} tracks?`}
          buttons={[
            { label: `Change ${changed} tracks`, primary: true, onClick: () => void apply() },
            { label: "Cancel", onClick: () => setConfirming(false) },
          ]}
        >
          <p className="mt-0">
            This edits {changed} of the {n} selected tracks in your library:{" "}
            {changeSummary(form).join(", ")}. It cannot be undone.
          </p>
        </ConfirmDialog>
      )}
    </form>
  );
}

/** The panel for 2+ selected tracks, with its loading, error and empty states. */
export function BulkEditPanel() {
  const { rows, pending } = useSelectedTracks();
  const error = useLibrary((s) => s.error);
  return (
    <InfoPanel>
      {pending > 0 ? (
        error !== null ? (
          <ErrorMessage message={error} />
        ) : (
          <p className={infoMuted}>Loading {pending} selected tracks…</p>
        )
      ) : rows.length === 0 ? (
        <p className={infoMuted}>Select tracks to edit them together.</p>
      ) : (
        <BulkEditForm key={rows.map((r) => r.id).join(",")} rows={rows} />
      )}
    </InfoPanel>
  );
}
