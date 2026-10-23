import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  reportCanSend: vi.fn(),
  reportNewCrash: vi.fn(),
  reportSend: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const reportSend = vi.mocked(ipc.reportSend);
vi.mocked(ipc.reportCanSend).mockResolvedValue(true);
const { isComplete, offerCrashReport, reportAction, useReport } = await import("./report");
const { useToasts } = await import("./toasts");

describe("report store", () => {
  beforeEach(() => {
    reportSend.mockReset();
    useToasts.setState({ toast: null });
  });

  it("opens a bug with diagnostics on and the error as context", async () => {
    await useReport.getState().openReport({ context: "boom" });
    const s = useReport.getState();
    expect(s.open).toBe(true);
    expect(s.canSend).toBe(true);
    expect(s.draft).toMatchObject({ kind: "bug", context: "boom", includeDiagnostics: true });
  });

  it("turns diagnostics off for feature requests", async () => {
    await useReport.getState().openReport();
    useReport.getState().setKind("feature");
    expect(useReport.getState().draft.includeDiagnostics).toBe(false);
  });

  it("sends only a complete draft", async () => {
    await useReport.getState().openReport();
    await useReport.getState().send();
    expect(reportSend).not.toHaveBeenCalled();

    useReport.getState().update({ title: "Crash", description: "On export" });
    expect(isComplete(useReport.getState().draft)).toBe(true);
    reportSend.mockResolvedValue();
    await useReport.getState().send();
    expect(useReport.getState()).toMatchObject({ phase: "sent", error: null });
  });

  it("keeps the draft and the reason when sending fails", async () => {
    await useReport.getState().openReport();
    useReport.getState().update({ title: "T", description: "D" });
    reportSend.mockRejectedValue(new Error("offline"));
    await useReport.getState().send();
    expect(useReport.getState()).toMatchObject({ phase: "failed", error: "offline" });
    expect(useReport.getState().draft.title).toBe("T");
  });

  it("offers a report after a crash, once", async () => {
    vi.mocked(ipc.reportNewCrash).mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    await offerCrashReport();
    expect(useToasts.getState().toast?.action?.label).toBe("Report");
    useToasts.setState({ toast: null });
    await offerCrashReport();
    expect(useToasts.getState().toast).toBeNull();
  });

  it("report action opens a bug about the message", () => {
    reportAction("disk full")?.run();
    expect(useReport.getState().draft.context).toBe("disk full");
  });
});
