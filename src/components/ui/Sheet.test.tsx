// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SLIDE_IN_DEFAULT as SLIDE_IN, SLIDE_OUT_DEFAULT as SLIDE_OUT } from "../../lib/motion";
import { Sheet } from "./Sheet";

/** jsdom has no Web Animations; record calls and finish by hand. */
interface FakeAnimation {
  keyframes: Keyframe[];
  options: KeyframeAnimationOptions;
  onfinish: (() => void) | null;
  cancel: () => void;
}
let started: FakeAnimation[] = [];
let running: FakeAnimation[] = [];

beforeEach(() => {
  started = [];
  running = [];
  HTMLElement.prototype.animate = (keyframes, options) => {
    const a: FakeAnimation = {
      keyframes: keyframes as Keyframe[],
      options: options as KeyframeAnimationOptions,
      onfinish: null,
      cancel: () => {
        running = running.filter((r) => r !== a);
      },
    };
    started.push(a);
    running.push(a);
    return a as unknown as Animation;
  };
  HTMLElement.prototype.getAnimations = () => [...running] as unknown as Animation[];
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function nth(list: FakeAnimation[], i: number): FakeAnimation {
  const a = list[i];
  if (a === undefined) throw new Error(`no animation ${i}`);
  return a;
}
const panel = () => document.querySelector('[data-slot="sheet"]');
const transforms = (a: FakeAnimation) => a.keyframes.map((k) => k.transform);

describe("Sheet", () => {
  it("slides in from off-screen as soon as it opens", () => {
    const { rerender } = render(<Sheet open={false}>info</Sheet>);
    expect(panel()).toBeNull();
    rerender(<Sheet open>info</Sheet>);
    expect(screen.getByText("info")).toBeTruthy();
    expect(transforms(nth(started, 0))).toEqual(["translateX(100%)", "none"]);
    expect(nth(started, 0).options.easing).toBe(SLIDE_IN.easing);
  });

  it("slides out faster, then unmounts its children", () => {
    const { rerender } = render(<Sheet open>info</Sheet>);
    running = [];
    rerender(<Sheet open={false}>info</Sheet>);
    const out = nth(started, 1);
    expect(transforms(out)).toEqual(["none", "translateX(100%)"]);
    expect(out.options.duration).toBe(SLIDE_OUT.duration);
    expect(SLIDE_OUT.duration).toBeLessThan(SLIDE_IN.duration);
    expect(screen.getByText("info")).toBeTruthy();
    act(() => out.onfinish?.());
    expect(panel()).toBeNull();
  });

  it("reopening mid-slide cancels the slide-out and stays mounted", () => {
    const { rerender } = render(<Sheet open>info</Sheet>);
    running = [];
    rerender(<Sheet open={false}>info</Sheet>);
    rerender(<Sheet open>info</Sheet>);
    expect(running).toHaveLength(1);
    expect(transforms(nth(running, 0)).at(-1)).toBe("none");
    expect(panel()).toBeTruthy();
  });
});
