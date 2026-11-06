/**
 * Settings → Export (D42): how reks writes sticks, set once instead of on
 * every export. The player a stick must work on belongs to the stick
 * (Sticks view, "Plays on"). Play history is always backed up, so it is
 * information here, not a switch. The experimental options share one
 * UNVERIFIED badge.
 */
import type { AnlzPreference, ExportSettings as Export, MissingAnalysis } from "../../lib/ipc";
import { useSettings } from "../../store/settings";
import { Badge } from "../ui/Badge";
import { Field } from "../ui/Field";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import { SettingItem, settingItemClass } from "./SettingItem";

/** `exportWithout` is left out: the writer refuses it (baken writes ANLZ for every track). */
const ANLZ_SOURCES: { value: AnlzPreference; label: string }[] = [
  { value: "rekordboxFirst", label: "rekordbox's, then reks's" },
  { value: "reksFirst", label: "reks's, then rekordbox's" },
  { value: "rekordboxOnly", label: "rekordbox's only" },
];

const MISSING: { value: MissingAnalysis; label: string }[] = [
  { value: "generate", label: "Export with approximate analysis" },
  { value: "skip", label: "Leave them out" },
];

export function ExportSettings({
  value,
  anlzPreference,
  keyBuiltin,
}: {
  value: Export;
  anlzPreference: AnlzPreference;
  keyBuiltin: boolean;
}) {
  const set = (patch: Partial<Export>) =>
    void useSettings.getState().update({ export: { ...value, ...patch } });
  return (
    <>
      <SettingItem
        title="Analysis to write"
        hint="Waveforms, beatgrid and cues when both reks and rekordbox analysed a track. A beatgrid you edited or locked always uses reks's unless rekordbox's only is chosen. reks's analysis is not yet tested on players."
      >
        <Select
          aria-label="Analysis to write"
          value={anlzPreference}
          onChange={(e) =>
            void useSettings.getState().update({ anlzPreference: e.target.value as AnlzPreference })
          }
        >
          {ANLZ_SOURCES.map((a) => (
            <option key={a.value} value={a.value}>
              {a.label}
            </option>
          ))}
        </Select>
      </SettingItem>
      <SettingItem
        title="Tracks without analysis"
        hint="Neither has analysed them. Approximate analysis is a quick waveform and beatgrid."
      >
        <Select
          aria-label="Tracks without analysis"
          value={value.missingAnalysis}
          onChange={(e) => set({ missingAnalysis: e.target.value as MissingAnalysis })}
        >
          {MISSING.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </Select>
      </SettingItem>
      <Toggle
        label="Artwork"
        hint="Cover art for the players to show."
        checked={value.artwork}
        onChange={(artwork) => set({ artwork })}
      />
      <Toggle
        label="Clean up unused files"
        hint="Remove music and analysis the stick no longer needs. The review lists them first."
        checked={value.cleanUp}
        onChange={(cleanUp) => set({ cleanUp })}
      />
      <SettingItem
        title="Play history"
        hint="Always backed up to your library's data folder before an export, so no export loses what the players logged."
      >
        <p className="m-0 text-xs text-fg-muted">
          Sessions you haven't imported are asked about in the review.
        </p>
      </SettingItem>
      <div className={settingItemClass}>
        <h5 className="m-0 flex items-center gap-2 text-sm font-semibold">
          Experimental
          <Badge variant="muted" title="Not yet tested on a CDJ or XDJ">
            UNVERIFIED
          </Badge>
        </h5>
        <p className="mt-0.5 mb-0 text-xs text-fg-muted">Not yet tested on a CDJ or XDJ.</p>
      </div>
      <Toggle
        label="Playlist dividers"
        hint="Each divider as a few seconds of silence titled with its label."
        checked={value.dividers}
        onChange={(dividers) => set({ dividers })}
      />
      <Toggle
        label="Start the first loop automatically"
        hint="The player locks into the track's first active memory loop when it reaches it, without pressing LOOP."
        checked={value.activeLoops}
        onChange={(activeLoops) => set({ activeLoops })}
      />
      <Toggle
        label="OneLibrary"
        hint={
          keyBuiltin
            ? "Also write exportLibrary.db for newer players. Not supported by AlphaTheta. Turned off, a stick's exportLibrary.db is deleted (the review asks first)."
            : "Also write exportLibrary.db for newer players. Needs a release build (or REKS_ONELIBRARY_KEY); without one only export.pdb is written."
        }
        checked={value.onelibrary}
        onChange={(onelibrary) => set({ onelibrary })}
      />
    </>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <div className={settingItemClass}>
      <Field label={label} orientation="horizontal" hint={hint}>
        <Switch size="md" checked={checked} onCheckedChange={onChange} />
      </Field>
    </div>
  );
}
