/**
 * App header: the top bar across the whole window for library-wide actions
 * (scan, import, sticks, report a problem, settings), with the scan status
 * line under it.
 * "Scan folder…" adds the folder to the library folders (Settings) and scans
 * it. "Import rekordbox XML…" opens the import wizard (`ImportWizard`), which
 * says how to export the XML and picks the file. USB export has one entry
 * point (D42): "Sticks" opens the Sticks view, with a count of plugged-in
 * drives; with exactly one known stick plugged in it reads "Sync <name>"
 * and opens the view on that stick. While the converter's dialog is hidden, a button shows its
 * progress and reopens it.
 */

import { pickFolder } from "../lib/ipc";
import { useConverter } from "../store/converter";
import { deviceName, useDevices } from "../store/devices";
import { useImportWizard } from "../store/importWizard";
import { useProfiles } from "../store/profiles";
import { useReport } from "../store/report";
import { useScan } from "../store/scan";
import { useSettings } from "../store/settings";
import { FeedbackIcon, GearIcon, UsbIcon } from "./icons";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { Tooltip } from "./ui/Tooltip";
import { useStickWatch } from "./useStickWatch";

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
    const dir = await pickFolder("Scan music folder");
    if (!dir) return;
    const root = await useSettings.getState().addRoot(dir);
    await useScan.getState().scan([root?.path ?? dir]);
    await useSettings.getState().loadRoots();
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
        <Button
          size="sm"
          disabled={scanning || importing}
          onClick={() => useImportWizard.getState().begin()}
        >
          Import rekordbox XML…
        </Button>
        <SticksButton disabled={importing} />
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

function openSticks(selectId?: number) {
  if (selectId !== undefined) useProfiles.getState().select(selectId);
  useDevices.getState().setDialogOpen(true);
}

/** The one way into USB export: "Sticks", or "Sync <name>" with one known stick plugged in. */
function SticksButton({ disabled }: { disabled: boolean }) {
  const { devices, only, onlyKnown } = useStickWatch();
  if (only !== null && onlyKnown !== null) {
    return (
      <Button size="sm" disabled={disabled} onClick={() => openSticks(onlyKnown.id)}>
        <UsbIcon />
        Sync {onlyKnown.name}
      </Button>
    );
  }
  return (
    <Button
      size="sm"
      disabled={disabled}
      aria-label={devices.length > 1 ? `Sticks, ${devices.length} plugged in` : undefined}
      onClick={() => openSticks()}
    >
      <UsbIcon />
      {only !== null ? `Set up ${deviceName(only)}` : "Sticks"}
      {devices.length > 1 && (
        <Badge variant="accent" aria-hidden="true">
          {devices.length}
        </Badge>
      )}
    </Button>
  );
}
