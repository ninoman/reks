/**
 * The Filters drawer's Save… (D35, wireframe S3): one name, and what to keep
 * the drawer as: a saved filter (in the drawer, narrows whatever is shown)
 * or a smart playlist (in the tree, written to USB as a fixed list). The
 * name starts as a suggestion from the rows, selected; the choice is
 * remembered (settings `ui.filterSaveTarget`). A saved-filter name already
 * taken asks to replace that one instead.
 */
import { type FormEvent, useEffect, useRef, useState } from "react";

import type { FilterPreset, FilterSaveTarget } from "../lib/ipc";
import { presetNamed, useFilterPresets } from "../store/filterPresets";
import { usePlaylists } from "../store/playlists";
import { useRules } from "../store/rules";
import { suggestName } from "../store/ruleTree";
import { Button } from "./ui/Button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/Dialog";
import { Field } from "./ui/Field";
import { Input } from "./ui/Input";
import { Segment, SegmentedControl } from "./ui/SegmentedControl";
import { useSummaryNames } from "./useSummaryNames";

const TARGET_TEXT: Record<FilterSaveTarget, string> = {
  filter: "Kept in this drawer. Load it in one click; it narrows whatever list you're looking at.",
  smart:
    "A playlist in the tree that updates by itself. When you export, it is written as a fixed list of the tracks it matches then.",
};

export function SaveFilterDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const target = useRules((s) => s.saveTarget);
  const playlistError = usePlaylists((s) => s.error);
  // A ref: the names object changes with every render, the suggestion only on opening.
  const summaryNames = useSummaryNames();
  const names = useRef(summaryNames);
  names.current = summaryNames;
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  /** The saved filter with this name, once the user was told about it. */
  const [clash, setClash] = useState<FilterPreset | null>(null);

  // Each opening starts from a name suggested by the rows.
  useEffect(() => {
    if (!open) return;
    setName(suggestName(useRules.getState().root, names.current));
    setFailed(false);
    setClash(null);
  }, [open]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    let saved: boolean;
    if (target === "filter") {
      const existing = presetNamed(useFilterPresets.getState().presets, name);
      if (existing !== null && clash?.id !== existing.id) {
        setClash(existing);
        return;
      }
      setBusy(true);
      saved = await useRules.getState().savePreset(name, existing ?? undefined);
    } else {
      setBusy(true);
      saved = await useRules.getState().saveAsSmart(name);
    }
    setBusy(false);
    setFailed(!saved);
    if (saved) onOpenChange(false);
  }

  let error: string | undefined;
  if (target === "filter" && clash !== null) {
    error = `“${clash.name}” already exists. Save again to replace it.`;
  } else if (target === "smart" && failed) {
    error = playlistError ?? undefined;
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="sm">
        <form onSubmit={(e) => void submit(e)}>
          <DialogHeader>
            <DialogTitle>Save filters</DialogTitle>
            <DialogDescription>{TARGET_TEXT[target]}</DialogDescription>
          </DialogHeader>
          <DialogBody>
            <Field label="Name" error={error}>
              <Input
                autoFocus
                value={name}
                onFocus={(e) => e.currentTarget.select()}
                onChange={(e) => {
                  setName(e.target.value);
                  setClash(null);
                }}
              />
            </Field>
            {/* Not in a Field: a <label> would name the first segment, not the group. */}
            <SegmentedControl
              size="sm"
              aria-label="Keep it as"
              value={target}
              onValueChange={(v) => {
                useRules.getState().setSaveTarget(v as FilterSaveTarget);
                setClash(null);
                setFailed(false);
              }}
            >
              <Segment value="filter">Saved filter</Segment>
              <Segment value="smart">Smart playlist</Segment>
            </SegmentedControl>
          </DialogBody>
          <DialogFooter>
            <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={busy || name.trim() === ""}>
              {clash !== null ? "Replace" : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
