// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  reportCanSend: vi.fn().mockResolvedValue(true),
}));

const { ErrorMessage } = await import("./ErrorMessage");
const { useReport } = await import("../store/report");

describe("ErrorMessage", () => {
  it("announces only the message and reports it as a bug", () => {
    render(<ErrorMessage message="disk full" />);
    expect(screen.getByRole("alert").textContent).toBe("disk full");
    fireEvent.click(screen.getByRole("button", { name: "Report" }));
    expect(useReport.getState()).toMatchObject({
      open: true,
      draft: { kind: "bug", context: "disk full", includeDiagnostics: true },
    });
  });
});
