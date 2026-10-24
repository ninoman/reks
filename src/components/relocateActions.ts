/** Entry points: pick the folder with the system dialog, then open the dialog for it. */
import { open } from "@tauri-apps/plugin-dialog";

import type { LibraryRoot, RelocateScope } from "../lib/ipc";
import { useRelocate } from "../store/relocate";
import { useRemap } from "../store/remap";

/** "Find missing files in…": the missing tracks of `scope`, searched in a folder the user picks. */
export async function findMissingFiles(scope: RelocateScope): Promise<void> {
  const dir = await open({ directory: true, title: "Find missing files in…" });
  if (typeof dir === "string") await useRelocate.getState().start(scope, dir);
}

/** "Change folder…": where library folder `root` lives now. */
export async function changeLibraryFolder(root: LibraryRoot): Promise<void> {
  const dir = await open({
    directory: true,
    title: "Where is this music folder now?",
    defaultPath: root.path,
  });
  if (typeof dir === "string") await useRemap.getState().start(root, dir);
}
