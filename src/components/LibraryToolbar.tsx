/**
 * Toolbar: search, track count, column chooser, and the minimal "fill the
 * library" actions (scan a folder, import a rekordbox XML). The full import
 * wizard is phase 1 task 12.
 */
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { rbxmlImportStart, scanCancel, scanStart, type PrefixRemap } from "../lib/ipc";
import { useLibrary } from "../store/library";
import { ColumnChooser } from "./ColumnChooser";
import { SearchBox } from "./SearchBox";
import type { TrackTableModel } from "./trackTableModel";

export function LibraryToolbar({ table }: { table: TrackTableModel }) {
  const total = useLibrary((s) => s.total);
  const selected = useLibrary((s) => s.selection.selected.size);
  const setText = useLibrary((s) => s.setText);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState<"scan" | "import" | null>(null);
  const [importOpen, setImportOpen] = useState(false);

  async function afterChange() {
    const { refresh, loadFacets } = useLibrary.getState();
    refresh();
    await loadFacets();
  }

  async function scanFolder() {
    const dir = await open({ directory: true, title: "Scan music folder" });
    if (typeof dir !== "string") return;
    setBusy("scan");
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
      setBusy(null);
      await afterChange();
    }
  }

  async function importXml(path: string, remap: PrefixRemap[]) {
    setBusy("import");
    setStatus("Importing…");
    try {
      const r = await rbxmlImportStart(path, remap, (e) => {
        if (e.type === "progress") setStatus(`Importing (${e.stage}) ${e.done}/${e.total}`);
      });
      const matched = r.matchedByPath + r.matchedByLocation + r.matchedByFallback;
      setStatus(
        `Import done in ${r.elapsedMs} ms: ${r.total} tracks, ${r.added} added, ${matched} matched, ` +
          `${r.unmatched.length} unmatched (${r.missing} missing files), ${r.warnings.length} warnings`,
      );
      setImportOpen(false);
    } catch (e) {
      setStatus(`Import failed: ${String(e)}`);
    } finally {
      setBusy(null);
      await afterChange();
    }
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
        <button type="button" disabled={busy !== null} onClick={() => void scanFolder()}>
          Scan folder…
        </button>
        {busy === "scan" && (
          <button type="button" onClick={() => void scanCancel()}>
            Stop
          </button>
        )}
        <button type="button" disabled={busy !== null} onClick={() => setImportOpen(!importOpen)}>
          Import rekordbox XML…
        </button>
      </div>
      {importOpen && (
        <ImportForm disabled={busy !== null} onImport={(p, r) => void importXml(p, r)} />
      )}
      {status !== "" && (
        <div className="status-line" role="status">
          {status}
        </div>
      )}
    </div>
  );
}

function ImportForm({
  disabled,
  onImport,
}: {
  disabled: boolean;
  onImport: (path: string, remap: PrefixRemap[]) => void;
}) {
  const [path, setPath] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");

  async function chooseXml() {
    const p = await open({
      title: "rekordbox collection.xml",
      filters: [{ name: "rekordbox XML", extensions: ["xml"] }],
    });
    if (typeof p === "string") setPath(p);
  }
  async function chooseTo() {
    const p = await open({ directory: true, title: "Folder the music is in now" });
    if (typeof p === "string") setTo(p);
  }
  const hasFrom = from.trim() !== "";
  const hasTo = to.trim() !== "";
  // A half-filled remap would be silently dropped; make the user finish or clear it.
  const remapIncomplete = hasFrom !== hasTo;
  const remap = hasFrom && hasTo ? [{ from: from.trim(), to: to.trim() }] : [];

  return (
    <form
      className="import-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (path !== "" && !remapIncomplete) onImport(path, remap);
      }}
    >
      <label>
        XML file
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="/path/to/collection.xml"
        />
        <button type="button" onClick={() => void chooseXml()}>
          Choose…
        </button>
      </label>
      <label>
        Map folder from
        <input
          value={from}
          onChange={(e) => setFrom(e.target.value)}
          placeholder="path prefix in the XML (optional)"
        />
      </label>
      <label>
        to
        <input
          value={to}
          onChange={(e) => setTo(e.target.value)}
          placeholder="where the files are now"
        />
        <button type="button" onClick={() => void chooseTo()}>
          Choose…
        </button>
      </label>
      <button type="submit" disabled={disabled || path === "" || remapIncomplete}>
        Import
      </button>
      {remapIncomplete && (
        <span className="hint" role="alert">
          Fill in both folder fields, or clear them, to import.
        </span>
      )}
    </form>
  );
}
