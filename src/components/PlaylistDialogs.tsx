/**
 * The playlist dialogs: delete confirmation and the duplicate question from
 * "add tracks". Both only show what the store holds; reks-core decided what
 * counts as a duplicate.
 */
import { usePlaylists } from "../store/playlists";
import { ConfirmDialog } from "./ConfirmDialog";

export function DuplicateDialog() {
  const pending = usePlaylists((s) => s.pendingDuplicates);
  const resolve = usePlaylists((s) => s.resolveDuplicates);
  if (pending === null) return null;
  const { duplicateCount, requested } = pending.outcome;
  const all = duplicateCount >= requested;
  return (
    <ConfirmDialog
      title="Duplicate tracks"
      buttons={[
        { label: "Add anyway", onClick: () => void resolve("allow"), primary: all },
        ...(all
          ? []
          : [{ label: "Skip duplicates", onClick: () => void resolve("skip"), primary: true }]),
        { label: "Cancel", onClick: () => void resolve(null) },
      ]}
    >
      {duplicateCount} of {requested} tracks are already in {pending.playlistName}
    </ConfirmDialog>
  );
}

export function DeleteDialog() {
  const node = usePlaylists((s) => s.confirmDelete);
  const resolve = usePlaylists((s) => s.resolveDelete);
  if (node === null) return null;
  return (
    <ConfirmDialog
      title={node.isFolder ? "Delete folder" : "Delete playlist"}
      buttons={[
        { label: "Delete", onClick: () => void resolve(true), primary: true },
        { label: "Cancel", onClick: () => void resolve(false) },
      ]}
    >
      {node.isFolder
        ? `Delete the folder “${node.name}” and everything in it?`
        : `Delete the playlist “${node.name}”?`}{" "}
      The tracks stay in the library.
    </ConfirmDialog>
  );
}
