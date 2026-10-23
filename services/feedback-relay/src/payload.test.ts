import { describe, expect, it } from "vitest";

import { MAX_BODY, parseReport, toDraft } from "./payload";

const good = { kind: "bug", title: " Export stops ", body: "### Description\n\nAt 40%." };

describe("parseReport", () => {
  it("accepts a well-formed report and trims the title", () => {
    expect(parseReport(good, "0.1.0")).toEqual({
      ok: true,
      report: { kind: "bug", title: "Export stops", body: good.body },
    });
  });

  it("requires the app version header", () => {
    expect(parseReport(good, null).ok).toBe(false);
    expect(parseReport(good, "latest").ok).toBe(false);
    expect(parseReport(good, "0.2.0-beta.1").ok).toBe(true);
  });

  it("rejects unknown kinds, blank fields and oversized bodies", () => {
    expect(parseReport({ ...good, kind: "spam" }, "0.1.0").ok).toBe(false);
    expect(parseReport({ ...good, title: "  " }, "0.1.0").ok).toBe(false);
    expect(parseReport({ ...good, body: "x".repeat(MAX_BODY + 1) }, "0.1.0").ok).toBe(false);
    expect(parseReport("nope", "0.1.0").ok).toBe(false);
  });
});

describe("toDraft", () => {
  it("notes the app version under the body", () => {
    const draft = toDraft({ kind: "feature", title: "T", body: "B" }, "0.1.0");
    expect(draft).toEqual({ title: "T", body: expect.stringMatching(/^B\n[\s\S]*reks 0\.1\.0/) });
  });
});
