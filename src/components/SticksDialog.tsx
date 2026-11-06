/**
 * One dialog for the Sticks view and the export (D42): Sync swaps the
 * content in place and the dialog animates to its new size, instead of one
 * dialog closing and another opening. The export wins while it is open; it
 * is hidden while one of its fixes runs in another dialog.
 */
import { useDevices } from "../store/devices";
import { useExportWizard } from "../store/exportWizard";
import { SticksPanel } from "./DevicesDialog";
import { ExportPanel } from "./ExportWizard";
import { Dialog, DialogContent } from "./ui/Dialog";

export function SticksDialog() {
  const sticksOpen = useDevices((s) => s.dialogOpen);
  const exporting = useExportWizard((s) => s.step !== "closed");
  const away = useExportWizard((s) => s.away !== null);
  if (exporting ? away : !sticksOpen) return null;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (open) return;
        if (exporting) useExportWizard.getState().close();
        else useDevices.getState().setDialogOpen(false);
      }}
    >
      <DialogContent size={exporting ? "lg" : "xl"} height={exporting ? "md" : "lg"}>
        {exporting ? <ExportPanel /> : <SticksPanel />}
      </DialogContent>
    </Dialog>
  );
}
