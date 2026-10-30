/** Entry points: pick the folder with the system dialog, then open the dialog for it. */

import type { LibraryRoot, RelocateScope } from "../lib/ipc";
import { pickFolder } from "../lib/ipc";
import { useRelocate } from "../store/relocate";
import { useRemap } from "../store/remap";

/** "Find missing files in…": the missing tracks of `scope`, searched in a folder the user picks. */
export async function findMissingFiles(scope: RelocateScope): Promise<void> {
  const dir = await pickFolder("Find missing files in…");
  if (dir) await useRelocate.getState().start(scope, dir);
}

/** "Change folder…": where library folder `root` lives now. */
export async function changeLibraryFolder(root: LibraryRoot): Promise<void> {
  const dir = await pickFolder("Where is this music folder now?", root.path);
  if (dir) await useRemap.getState().start(root, dir);
}
