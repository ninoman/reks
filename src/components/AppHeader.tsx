/**
 * App header: the top bar across the whole window for library-wide actions
 * (scan, import, export to USB, USB devices, report a problem, settings), with the scan status
 * line under it.
 * "Scan folder…" adds the folder to the library folders (Settings) and scans
 * it. "Import rekordbox XML…" picks the file and opens the import wizard
 * (`ImportWizard`). "Export to USB…" opens the export wizard (`ExportWizard`)
 * with the playlist selected in the sidebar ticked. While the converter's
 * dialog is hidden, a button shows its progress and reopens it.
 */

import { open } from "@tauri-apps/plugin-dialog";

import { useConverter } from "../store/converter";
import { useDevices } from "../store/devices";
import { openExportWizard } from "../store/exportWizard";
import { useImportWizard } from "../store/importWizard";
import { useReport } from "../store/report";
import { useScan } from "../store/scan";
import { useSettings } from "../store/settings";
import { FeedbackIcon, GearIcon, UsbIcon } from "./icons";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { Tooltip } from "./ui/Tooltip";

export function AppHeader() {
  const status = useScan((s) => s.status);
  const scanning = useScan((s) => s.scanning);
  const importing = useImportWizard((s) => s.step !== "closed");
  const converting = useConverter((s) =>
    !s.hidden
      ? null
      : s.step === "report"
        ? "Conversion done"
        : s.progress === null
          ? "Converting…"
          : `Converting ${s.progress.index + 1} of ${s.progress.total}…`,
  );

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
    <header>
      <div className="flex h-(--toolbar-h) items-center gap-2 border-b border-border bg-bg-subtle px-3">
        <span className="flex-1 font-medium tracking-wide">reks</span>
        {converting !== null && (
          <Button size="sm" onClick={() => useConverter.getState().setHidden(false)}>
            {converting}
          </Button>
        )}
        <Button size="sm" disabled={scanning || importing} onClick={() => void scanFolder()}>
          Scan folder…
        </Button>
        {scanning && (
          <Button size="sm" onClick={() => useScan.getState().cancel()}>
            Stop
          </Button>
        )}
        <Button size="sm" disabled={scanning || importing} onClick={() => void chooseXml()}>
          Import rekordbox XML…
        </Button>
        <Button size="sm" disabled={importing} onClick={() => openExportWizard()}>
          Export to USB…
        </Button>
        <Tooltip content="USB devices">
          <IconButton
            size="sm"
            aria-label="USB devices"
            onClick={() => useDevices.getState().setDialogOpen(true)}
          >
            <UsbIcon />
          </IconButton>
        </Tooltip>
        <Tooltip content="Report a problem or idea">
          <IconButton
            size="sm"
            aria-label="Report a problem or idea"
            onClick={() => void useReport.getState().openReport()}
          >
            <FeedbackIcon />
          </IconButton>
        </Tooltip>
        <Tooltip content="Settings" shortcut="⌘ ,">
          <IconButton
            size="sm"
            aria-label="Settings"
            onClick={() => useSettings.getState().setDialogOpen(true)}
          >
            <GearIcon />
          </IconButton>
        </Tooltip>
      </div>
      {status !== "" && (
        <div
          className="border-b border-border px-3 py-1 text-xs text-fg-muted select-text"
          role="status"
        >
          {status}
        </div>
      )}
    </header>
  );
}
