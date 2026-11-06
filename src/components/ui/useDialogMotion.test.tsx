// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { useDialogMotion } from "./useDialogMotion";

let animate: ReturnType<typeof vi.fn>;

beforeEach(() => {
  animate = vi.fn(() => ({ onfinish: null, oncancel: null }));
  HTMLElement.prototype.animate = animate as unknown as HTMLElement["animate"];
  vi.useFakeTimers();
  document.body.innerHTML = "";
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function Shown() {
  const ref = useDialogMotion();
  return (
    <div>
      <div data-testid="backdrop" />
      <div ref={ref} role="dialog" id="d">
        hello
      </div>
    </div>
  );
}

it("fades the dialog and its backdrop in", () => {
  render(<Shown />);
  expect(animate).toHaveBeenCalledTimes(2);
  expect(animate.mock.calls[0]?.[0]).toEqual([{ opacity: 0 }, { opacity: 1 }]);
});

it("leaves an inert copy to fade out on close, then removes it", async () => {
  const { unmount } = render(<Shown />);
  unmount();
  await Promise.resolve();
  const ghosts = [...document.body.querySelectorAll(":scope > [aria-hidden='true']")];
  expect(ghosts).toHaveLength(2);
  expect(ghosts.every((g) => (g as HTMLElement).style.pointerEvents === "none")).toBe(true);
  expect(document.querySelector("[role='dialog']")).toBeNull();
  vi.runAllTimers();
  expect(document.body.querySelectorAll(":scope > [aria-hidden='true']")).toHaveLength(0);
});
