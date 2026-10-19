/**
 * Toolbar: search, track count, column chooser, and the "fill the library"
 * actions. "Import rekordbox XML…" picks the file and opens the import
 * wizard (`ImportWizard`); folder scans report on the status line.
 */

import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { scanCancel, scanStart } from "../lib/ipc";
import { useImportWizard } from "../store/importWizard";
import { useLibrary } from "../store/library";
import { usePlaylists } from "../store/playlists";
import { ColumnChooser } from "./ColumnChooser";
import { SearchBox } from "./SearchBox";
import type { TrackTableModel } from "./trackTableModel";

export function LibraryToolbar({ table }: { table: TrackTableModel }) {
  const total = useLibrary((s) => s.total);
  const selected = useLibrary((s) => s.selection.selected.size);
  const setText = useLibrary((s) => s.setText);
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
    <div className="toolbar-wrap">
      <div className="toolbar">
        <SearchBox onSearch={setText} />
        <span className="count">
          {total.toLocaleString()} tracks
          {selected > 0 ? ` · ${selected.toLocaleString()} selected` : ""}
        </span>
        <span className="spacer" />
        <ColumnChooser table={table} />
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
    </div>
  );
}
