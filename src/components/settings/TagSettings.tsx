/** Settings → Library: write My Tags to Comments (rekordbox's "Add My Tag to Comments", D36). */
import { useSettings } from "../../store/settings";
import { ErrorMessage } from "../ErrorMessage";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Switch } from "../ui/Switch";
import { settingItemClass } from "./SettingItem";
import { useCommentSync } from "./useCommentSync";

export function TagSettings({ on }: { on: boolean }) {
  const sync = useCommentSync(on);
  return (
    <div className={settingItemClass}>
      <Field
        label="Write My Tags to Comments"
        orientation="horizontal"
        hint={
          'Every tag edit rewrites the track\'s comment to end with its tags as "/* Tag A / Tag B */", like rekordbox\'s "Add My Tag to Comments". Text before the block is kept. Off by default.'
        }
      >
        <Switch
          size="md"
          checked={on}
          onCheckedChange={(next) => void useSettings.getState().update({ tagsInComments: next })}
        />
      </Field>
      {on && (
        <div className="mt-2.5 flex items-center gap-2 text-xs text-fg-muted">
          {sync.error !== null ? (
            <ErrorMessage as="span" message={sync.error} />
          ) : sync.pending === null ? (
            <span>Checking comments…</span>
          ) : sync.pending === 0 ? (
            <span>Every tagged track's comment is up to date.</span>
          ) : (
            <>
              <span>
                {sync.pending.toLocaleString()} tagged track
                {sync.pending === 1 ? "'s comment differs" : "s' comments differ"} from the tags.
              </span>
              <Button size="sm" disabled={sync.busy} onClick={() => void sync.apply()}>
                Update now
              </Button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
