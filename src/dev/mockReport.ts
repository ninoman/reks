/**
 * Dev-only report commands for the mock backend (`mockBackend.ts`): renders
 * a body like `reks_core::diagnostics::render` and "sends" it after a delay.
 */
import type { Report, ReportDraft } from "../lib/ipc";

function render(d: ReportDraft): Report {
  const parts = [`### Description\n\n${d.description}`];
  if (d.kind === "bug" && d.steps !== "") parts.push(`### Steps to reproduce\n\n${d.steps}`);
  if (d.context !== "") parts.push(`### Error shown\n\n${d.context}`);
  if (d.includeDiagnostics)
    parts.push("<details><summary>Diagnostics</summary>\n\n| reks | 0.1.0 (mock) |\n\n</details>");
  return { kind: d.kind, title: d.title, body: parts.join("\n\n") };
}

export function mockReport(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "report_can_send":
      return true;
    case "report_new_crash":
      return false;
    case "report_preview":
      return render(args.draft as ReportDraft);
    case "report_send":
      return new Promise((resolve) => setTimeout(() => resolve(null), 600));
    case "report_save":
      return null;
    case "log_ui_error":
      return null;
    default:
      return undefined;
  }
}
