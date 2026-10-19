// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SLIDE_IN_DEFAULT as SLIDE_IN, SLIDE_OUT_DEFAULT as SLIDE_OUT } from "../lib/motion";
import { SlideOver } from "./SlideOver";

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
const panel = () => document.querySelector(".slide-over");
const transforms = (a: FakeAnimation) => a.keyframes.map((k) => k.transform);

describe("SlideOver", () => {
  it("slides in from off-screen as soon as it opens", () => {
    const { rerender } = render(<SlideOver open={false}>info</SlideOver>);
    expect(panel()).toBeNull();
    rerender(<SlideOver open>info</SlideOver>);
    expect(screen.getByText("info")).toBeTruthy();
    expect(transforms(nth(started, 0))).toEqual(["translateX(100%)", "none"]);
    expect(nth(started, 0).options.easing).toBe(SLIDE_IN.easing);
  });

  it("slides out faster, then unmounts its children", () => {
    const { rerender } = render(<SlideOver open>info</SlideOver>);
    running = [];
    rerender(<SlideOver open={false}>info</SlideOver>);
    const out = nth(started, 1);
    expect(transforms(out)).toEqual(["none", "translateX(100%)"]);
    expect(out.options.duration).toBe(SLIDE_OUT.duration);
    expect(SLIDE_OUT.duration).toBeLessThan(SLIDE_IN.duration);
    expect(screen.getByText("info")).toBeTruthy();
    act(() => out.onfinish?.());
    expect(panel()).toBeNull();
  });

  it("reopening mid-slide cancels the slide-out and stays mounted", () => {
    const { rerender } = render(<SlideOver open>info</SlideOver>);
    running = [];
    rerender(<SlideOver open={false}>info</SlideOver>);
    rerender(<SlideOver open>info</SlideOver>);
    expect(running).toHaveLength(1);
    expect(transforms(nth(running, 0)).at(-1)).toBe("none");
    expect(panel()).toBeTruthy();
  });
});
