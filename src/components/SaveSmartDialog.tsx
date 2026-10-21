/** Names a new smart playlist ("Save as smart playlist…") and saves it. */
import { type FormEvent, useState } from "react";

import { usePlaylists } from "../store/playlists";
import { useRules } from "../store/rules";
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

export function SaveSmartDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const error = usePlaylists((s) => s.error);
  const [failed, setFailed] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    const saved = await useRules.getState().saveAsSmart(name);
    setBusy(false);
    setFailed(!saved);
    if (saved) {
      setName("");
      onOpenChange(false);
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) setFailed(false);
        onOpenChange(o);
      }}
    >
      <DialogContent size="sm">
        <form onSubmit={(e) => void submit(e)}>
          <DialogHeader>
            <DialogTitle>Save as smart playlist</DialogTitle>
            <DialogDescription>
              Its tracks follow the conditions, so it updates as your library changes. When you
              export, it is written as a fixed list of the tracks it matches then.
            </DialogDescription>
          </DialogHeader>
          <DialogBody>
            <Field label="Name" error={failed ? error : undefined}>
              <Input
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Peak, not warm-up"
              />
            </Field>
          </DialogBody>
          <DialogFooter>
            <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={busy || name.trim() === ""}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
