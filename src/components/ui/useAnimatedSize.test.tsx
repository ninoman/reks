// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { useAnimatedSize } from "./useAnimatedSize";

/** The one ResizeObserver callback the hook registers. */
let notify: () => void = () => {};
let size = { width: 960, height: 700 };

beforeEach(() => {
  size = { width: 960, height: 700 };
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: () => void) {
        notify = cb;
      }
      observe() {}
      disconnect() {}
    },
  );
  HTMLElement.prototype.getBoundingClientRect = () => ({
    ...size,
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    x: 0,
    y: 0,
    toJSON() {},
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function Box() {
  const ref = useAnimatedSize();
  return <div ref={ref} data-testid="box" style={{ maxHeight: "648px" }} />;
}

it("animates the height from the old size to the new one, its cap lifted until it ends", () => {
  const animation = { onfinish: null as null | (() => void), oncancel: null, cancel() {} };
  const animate = vi.fn(() => animation);
  HTMLElement.prototype.animate = animate as unknown as HTMLElement["animate"];
  const { getByTestId } = render(<Box />);
  const box = getByTestId("box");

  size = { width: 640, height: 320 };
  act(() => notify());
  expect(animate).toHaveBeenCalledTimes(1);
  const [frames] = animate.mock.calls[0] as unknown as [Keyframe[]];
  // Height only: the width changes at once (animating it rewraps text each frame).
  expect(frames).toEqual([{ height: "700px" }, { height: "320px" }]);
  expect(box.style.maxHeight).toBe("none");

  // Resizes while it runs are its own frames.
  size = { width: 700, height: 500 };
  act(() => notify());
  expect(animate).toHaveBeenCalledTimes(1);

  act(() => animation.onfinish?.());
  expect(box.style.maxHeight).toBe("648px");
});

it("a width-only change does not animate", () => {
  const animate = vi.fn();
  HTMLElement.prototype.animate = animate as unknown as HTMLElement["animate"];
  render(<Box />);
  size = { width: 640, height: 700 };
  act(() => notify());
  expect(animate).not.toHaveBeenCalled();
});

it("does nothing when the size did not change", () => {
  const animate = vi.fn();
  HTMLElement.prototype.animate = animate as unknown as HTMLElement["animate"];
  render(<Box />);
  act(() => notify());
  expect(animate).not.toHaveBeenCalled();
});
