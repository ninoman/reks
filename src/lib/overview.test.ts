import { describe, expect, it } from "vitest";

import { fitColumns, overviewColumns } from "./overview";

describe("overview", () => {
  it("reads whole pairs from the bytes", () => {
    const bytes = new Float32Array([-0.5, 0.5, -0.25, 0.75, 9]).buffer;
    expect([...overviewColumns(bytes)]).toEqual([-0.5, 0.5, -0.25, 0.75]);
  });

  it("takes the extremes of the columns under each pixel", () => {
    const cols = new Float32Array([-0.1, 0.2, -0.5, 0.1, -0.2, 0.9, 0, 0.3]);
    expect([...fitColumns(cols, 2)].map((v) => Math.round(v * 10) / 10)).toEqual([
      -0.5, 0.2, -0.2, 0.9,
    ]);
  });

  it("repeats columns when there are more pixels than columns", () => {
    const cols = new Float32Array([-0.5, 0.5]);
    expect([...fitColumns(cols, 3)]).toEqual([-0.5, 0.5, -0.5, 0.5, -0.5, 0.5]);
  });

  it("is flat with no columns", () => {
    expect([...fitColumns(new Float32Array(), 2)]).toEqual([0, 0, 0, 0]);
  });
});
