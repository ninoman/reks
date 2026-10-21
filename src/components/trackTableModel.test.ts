import { describe, expect, it } from "vitest";

import { fitColumnWidths } from "./trackTableModel";

const sum = (a: number[]) => a.reduce((x, y) => x + y, 0);

describe("fitColumnWidths", () => {
  const cols = [
    { width: 28 },
    { width: 300, grow: true as const },
    { width: 100, grow: true as const },
    { width: 70 },
  ];

  it("keeps base widths when the columns exceed the container", () => {
    expect(fitColumnWidths(cols, 400)).toEqual([28, 300, 100, 70]);
    expect(fitColumnWidths(cols, 498)).toEqual([28, 300, 100, 70]);
  });

  it("shares spare width among growing columns by width, filling exactly", () => {
    const out = fitColumnWidths(cols, 998);
    expect(sum(out)).toBe(998);
    expect(out[0]).toBe(28);
    expect(out[3]).toBe(70);
    expect(out[1]).toBe(675);
    expect(out[2]).toBe(225);
  });

  it("puts the rounding remainder on the last growing column", () => {
    const out = fitColumnWidths(
      [{ width: 10, grow: true }, { width: 10, grow: true }, { width: 5 }],
      28,
    );
    expect(out).toEqual([11, 12, 5]);
  });

  it("grows every column when none is flagged", () => {
    const out = fitColumnWidths([{ width: 100 }, { width: 100 }], 300);
    expect(out).toEqual([150, 150]);
  });

  it("handles an unmeasured container and no columns", () => {
    expect(fitColumnWidths(cols, 0)).toEqual([28, 300, 100, 70]);
    expect(fitColumnWidths([], 500)).toEqual([]);
  });
});
