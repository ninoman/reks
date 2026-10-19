/**
 * App header: the top bar across the whole window for library-wide actions
 * (scan, import, settings), with the scan status line under it.
 * "Scan folder…" adds the folder to the library folders (Settings) and scans
 * it. "Import rekordbox XML…" picks the file and opens the import wizard
 * (`ImportWizard`).
 */

import { open } from "@tauri-apps/plugin-dialog";

import { useImportWizard } from "../store/importWizard";
import { useScan } from "../store/scan";
import { useSettings } from "../store/settings";
import { GearIcon } from "./icons";

export function AppHeader() {
  const status = useScan((s) => s.status);
  const scanning = useScan((s) => s.scanning);
  const importing = useImportWizard((s) => s.step !== "closed");

  async function scanFolder() {
    const dir = await open({ directory: true, title: "Scan music folder" });
    if (typeof dir !== "string") return;
    const root = await useSettings.getState().addRoot(dir);
    await useScan.getState().scan([root?.path ?? dir]);
    await useSettings.getState().loadRoots();
  }

  async function chooseXml() {
    const path = await open({
      title: "Import rekordbox collection.xml",
      filters: [{ name: "rekordbox XML", extensions: ["xml"] }],
    });
    if (typeof path === "string") await useImportWizard.getState().open(path);
  }

  return (
    <header className="app-header-wrap">
      <div className="app-header">
        <span className="app-name">reks</span>
        <span className="spacer" />
        <button type="button" disabled={scanning || importing} onClick={() => void scanFolder()}>
          Scan folder…
        </button>
        {scanning && (
          <button type="button" onClick={() => useScan.getState().cancel()}>
            Stop
          </button>
        )}
        <button type="button" disabled={scanning || importing} onClick={() => void chooseXml()}>
          Import rekordbox XML…
        </button>
        <button
          type="button"
          className="header-icon"
          aria-label="Settings"
          title="Settings (⌘,)"
          onClick={() => useSettings.getState().setDialogOpen(true)}
        >
          <GearIcon />
        </button>
      </div>
      {status !== "" && (
        <div className="status-line" role="status">
          {status}
        </div>
      )}
    </header>
  );
}
