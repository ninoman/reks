/// <reference types="node" />
/** TS mirrors of values in tokens.css must match it. */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { ROW_HEIGHT } from "../components/trackTableModel";
import { SLIDE_IN_DEFAULT, SLIDE_OUT_DEFAULT } from "../lib/motion";

function token(name: string): string {
  const m = css.match(new RegExp(`${name}:\\s*([^;]+);`));
  if (m?.[1] === undefined) throw new Error(`${name} missing from tokens.css`);
  return m[1].trim();
}

// Read from disk: Vitest does not hand CSS through `?raw`.
const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

describe("tokens.css mirrors", () => {
  it("row height", () => {
    expect(token("--row-h")).toBe(`${ROW_HEIGHT}px`);
  });

  it("slide timings", () => {
    expect(token("--duration-slide-in")).toBe(`${SLIDE_IN_DEFAULT.duration}ms`);
    expect(token("--duration-slide-out")).toBe(`${SLIDE_OUT_DEFAULT.duration}ms`);
    expect(token("--ease-out-quint")).toBe(SLIDE_IN_DEFAULT.easing);
    expect(token("--ease-snap")).toBe(SLIDE_OUT_DEFAULT.easing);
  });

  it("sizes the decks and the panel from the mixer and the library", () => {
    // #46 changes --mixer-h per view; the 2-deck decks must follow it.
    expect(token("--deck-h")).toBe("var(--mixer-h)");
    expect(token("--mix-panel-max-h")).toContain("var(--library-min-h)");
    expect(token("--library-min-h")).toBe("246px");
  });

  it("keeps raw colours out of the semantic layer", () => {
    const theme = css.slice(css.indexOf("@theme static {"));
    const hex = theme.match(/#[0-9a-f]{3,8}\b/gi) ?? [];
    expect(hex).toEqual([]);
  });
});
