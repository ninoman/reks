/**
 * App header: the top bar across the whole window for library-wide actions
 * ("fill the library" now; more later), with the scan status line under it.
 * "Import rekordbox XML…" picks the file and opens the import wizard
 * (`ImportWizard`).
 */

import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { scanCancel, scanStart } from "../lib/ipc";
import { useImportWizard } from "../store/importWizard";
import { useLibrary } from "../store/library";
import { usePlaylists } from "../store/playlists";

export function AppHeader() {
  const [status, setStatus] = useState("");
  const [scanning, setScanning] = useState(false);
  const importing = useImportWizard((s) => s.step !== "closed");

  async function afterChange() {
    const { refresh, loadFacets } = useLibrary.getState();
    refresh();
    await Promise.all([loadFacets(), usePlaylists.getState().load()]);
  }

  async function scanFolder() {
    const dir = await open({ directory: true, title: "Scan music folder" });
    if (typeof dir !== "string") return;
    setScanning(true);
    setStatus("Scanning…");
    try {
      const r = await scanStart([dir], (e) => {
        if (e.type === "progress") setStatus(`Scanning ${e.done}/${e.total}`);
      });
      setStatus(
        `Scan ${r.cancelled ? "cancelled" : "done"}: ${r.added} added, ${r.updated} updated, ` +
          `${r.moved} moved, ${r.missing} missing, ${r.errors.length} errors`,
      );
    } catch (e) {
      setStatus(`Scan failed: ${String(e)}`);
    } finally {
      setScanning(false);
      await afterChange();
    }
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
          <button type="button" onClick={() => void scanCancel()}>
            Stop
          </button>
        )}
        <button type="button" disabled={scanning || importing} onClick={() => void chooseXml()}>
          Import rekordbox XML…
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
