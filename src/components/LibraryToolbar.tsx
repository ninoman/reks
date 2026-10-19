/**
 * Toolbar: search, track count, column chooser, and the minimal "fill the
 * library" actions. "Import rekordbox XML…" is one step: pick the file and it
 * imports. Only when files are not found does it offer to locate the folder
 * they are in now (re-importing with a prefix remap). The full import wizard
 * is phase 1 task 12.
 */
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  rbxmlImportStart,
  scanCancel,
  scanStart,
  type ImportReport,
  type PrefixRemap,
} from "../lib/ipc";
import { useLibrary } from "../store/library";
import { ColumnChooser } from "./ColumnChooser";
import { SearchBox } from "./SearchBox";
import type { TrackTableModel } from "./trackTableModel";

/** The last import, kept so missing files can be located and re-imported. */
interface LastImport {
  path: string;
  remap: PrefixRemap[];
  report: ImportReport;
}

function plural(n: number, word: string): string {
  return `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
}

function importSummary(r: ImportReport): string {
  const updated = r.matchedByPath + r.matchedByLocation + r.matchedByFallback;
  const parts = [`Imported ${plural(r.total, "track")}: ${r.added} new, ${updated} updated`];
  const skipped = r.unmatched.length - r.missing;
  if (skipped > 0) parts.push(`${plural(skipped, "entry")} skipped (no local file)`);
  if (r.warnings.length > 0) parts.push(plural(r.warnings.length, "warning"));
  return `${parts.join(", ")}.`;
}

export function LibraryToolbar({ table }: { table: TrackTableModel }) {
  const total = useLibrary((s) => s.total);
  const selected = useLibrary((s) => s.selection.selected.size);
  const setText = useLibrary((s) => s.setText);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState<"scan" | "import" | null>(null);
  const [lastImport, setLastImport] = useState<LastImport | null>(null);

  async function afterChange() {
    const { refresh, loadFacets } = useLibrary.getState();
    refresh();
    await loadFacets();
  }

  async function scanFolder() {
    const dir = await open({ directory: true, title: "Scan music folder" });
    if (typeof dir !== "string") return;
    setBusy("scan");
    setLastImport(null);
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

  async function runImport(path: string, remap: PrefixRemap[]) {
    setBusy("import");
    setLastImport(null);
    setStatus("Importing…");
    try {
      const report = await rbxmlImportStart(path, remap, (e) => {
        if (e.type === "progress") {
          const what = e.stage === "files" ? "Reading files" : "Saving";
          setStatus(`${what} ${e.done}/${e.total}`);
        }
      });
      setStatus(importSummary(report));
      setLastImport({ path, remap, report });
    } catch (e) {
      setStatus(`Import failed: ${String(e)}`);
    } finally {
      setBusy(null);
      await afterChange();
    }
  }

  async function chooseXml() {
    const path = await open({
      title: "Import rekordbox collection.xml",
      filters: [{ name: "rekordbox XML", extensions: ["xml"] }],
    });
    if (typeof path === "string") await runImport(path, []);
  }

  /** Re-imports with the missing files' folder mapped to where they are now. */
  async function locateMissing(last: LastImport, from: string) {
    const to = await open({ directory: true, title: `Where is "${from}" now?` });
    if (typeof to !== "string") return;
    await runImport(last.path, [{ from, to }, ...last.remap]);
  }

  const missing = lastImport?.report.missing ?? 0;
  const missingRoot = lastImport?.report.missingRoot ?? null;

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
        <button type="button" disabled={busy !== null} onClick={() => void chooseXml()}>
          Import rekordbox XML…
        </button>
      </div>
      {status !== "" && (
        <div className="status-line" role="status">
          {status}
          {lastImport !== null && missing > 0 && (
            <>
              {" "}
              <span className="warn">
                {plural(missing, "file")} not found
                {missingRoot !== null ? ` in ${missingRoot}` : ""}.
              </span>
              {missingRoot !== null && (
                <button
                  type="button"
                  className="link"
                  disabled={busy !== null}
                  onClick={() => void locateMissing(lastImport, missingRoot)}
                >
                  Locate folder…
                </button>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
