/**
 * The report the app sends (`reks_core::diagnostics::Report`) and the board item
 * made from it. Pure, so it is tested without the Workers runtime.
 */

export type ReportKind = "bug" | "feature" | "feedback";

export interface Report {
  kind: ReportKind;
  title: string;
  body: string;
}

/** A draft item on the board. */
export interface Draft {
  title: string;
  body: string;
}

/** GitHub's own limits are 256 title and 65,536 body characters. */
export const MAX_TITLE = 256;
export const MAX_BODY = 65_000;

const KINDS: readonly ReportKind[] = ["bug", "feature", "feedback"];
const VERSION = /^\d+\.\d+\.\d+([-+][\w.]+)?$/;

export type Parsed = { ok: true; report: Report } | { ok: false; error: string };

/** Checks the request body; `version` is the `X-Reks-Version` header. */
export function parseReport(json: unknown, version: string | null): Parsed {
  if (version === null || !VERSION.test(version)) return fail("missing or bad X-Reks-Version");
  if (typeof json !== "object" || json === null) return fail("body must be a JSON object");
  const { kind, title, body } = json as Record<string, unknown>;
  if (typeof kind !== "string" || !KINDS.includes(kind as ReportKind)) return fail("bad kind");
  if (typeof title !== "string" || title.trim() === "" || title.length > MAX_TITLE)
    return fail("bad title");
  if (typeof body !== "string" || body.trim() === "" || body.length > MAX_BODY)
    return fail("bad body");
  return { ok: true, report: { kind: kind as ReportKind, title: title.trim(), body } };
}

function fail(error: string): Parsed {
  return { ok: false, error };
}

/** The board item for a report; the kind goes in the Kind field. */
export function toDraft(report: Report, version: string): Draft {
  return {
    title: report.title,
    body: `${report.body}\n\n---\n_Sent from reks ${version} with the in-app report._`,
  };
}
