// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/uiErrors", () => ({ logError: vi.fn() }));
vi.mock("../store/report", () => ({
  useReport: { getState: () => ({ openReport: vi.fn() }) },
}));
vi.mock("./ReportDialog", () => ({ ReportDialog: () => null }));

const { ErrorBoundary } = await import("./ErrorBoundary");

let broken = true;
// biome-ignore lint/style/useComponentExportOnlyModules: test helper, not a Fast Refresh module
function Bomb() {
  if (broken) throw new Error("boom");
  return <p>recovered</p>;
}

describe("ErrorBoundary", () => {
  afterEach(() => {
    cleanup();
    broken = true;
  });

  it("a region failure leaves the rest of the screen alone and can retry", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <div>
        <ErrorBoundary region="Sidebar">
          <Bomb />
        </ErrorBoundary>
        <ErrorBoundary region="Library">
          <p>library is fine</p>
        </ErrorBoundary>
      </div>,
    );
    expect(screen.getByText("Sidebar hit a problem")).toBeTruthy();
    expect(screen.getByText("library is fine")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Reload" })).toBeNull();

    broken = false;
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByText("recovered")).toBeTruthy();
  });

  it("the app-wide boundary offers Reload", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Bomb />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
  });
});
