import { describe, expect, it } from "vitest";

import { fieldValues, type SelectField } from "./fields";

const cfg = { typeField: "Kind", statusField: "Status", statusValue: "Needs triage" };
const fields: SelectField[] = [
  {
    id: "F_type",
    name: "Kind",
    options: [
      { id: "o_bug", name: "Bug" },
      { id: "o_feat", name: "Feature" },
    ],
  },
  { id: "F_status", name: "status", options: [{ id: "o_triage", name: "needs TRIAGE" }] },
];

describe("fieldValues", () => {
  it("sets type and status, matching names without case", () => {
    expect(fieldValues(fields, "bug", cfg)).toEqual([
      { fieldId: "F_type", optionId: "o_bug" },
      { fieldId: "F_status", optionId: "o_triage" },
    ]);
  });

  it("skips an option or field the board lacks", () => {
    expect(fieldValues(fields, "feedback", cfg)).toEqual([
      { fieldId: "F_status", optionId: "o_triage" },
    ]);
    expect(fieldValues([], "bug", cfg)).toEqual([]);
  });
});
