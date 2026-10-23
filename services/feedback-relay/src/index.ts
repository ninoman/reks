/**
 * Feedback relay (D33): `POST /report` from the reks app becomes a draft
 * item on the maintainer's private GitHub Projects board, with its Kind and
 * Status set. Testers need no GitHub account; the token never leaves the
 * Worker.
 */
import { fieldValues } from "./fields";
import { addDraft, type Board, findBoard } from "./github";
import { MAX_BODY, parseReport, toDraft } from "./payload";

interface Env {
  PROJECT_OWNER: string;
  PROJECT_NUMBER: string;
  TYPE_FIELD: string;
  STATUS_FIELD: string;
  STATUS_VALUE: string;
  GITHUB_TOKEN: string;
  REPORT_LIMIT: RateLimit;
}

/** The board's id and fields, looked up once per isolate. */
let board: Promise<Board> | null = null;

function text(status: number, message: string): Response {
  return new Response(message, { status, headers: { "Content-Type": "text/plain" } });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname !== "/report") return text(404, "not found");
    if (request.method !== "POST") return text(405, "use POST");

    const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
    const { success } = await env.REPORT_LIMIT.limit({ key: ip });
    if (!success) return text(429, "too many reports, try again in a minute");

    const length = Number(request.headers.get("Content-Length") ?? "0");
    if (length > 2 * MAX_BODY) return text(413, "report too large");

    let json: unknown;
    try {
      json = await request.json();
    } catch {
      return text(400, "body must be JSON");
    }
    const version = request.headers.get("X-Reks-Version");
    const parsed = parseReport(json, version);
    if (!parsed.ok) return text(400, parsed.error);

    try {
      board ??= findBoard(env.GITHUB_TOKEN, env.PROJECT_OWNER, Number(env.PROJECT_NUMBER));
      const values = fieldValues((await board).fields, parsed.report.kind, {
        typeField: env.TYPE_FIELD,
        statusField: env.STATUS_FIELD,
        statusValue: env.STATUS_VALUE,
      });
      await addDraft(env.GITHUB_TOKEN, await board, toDraft(parsed.report, version ?? ""), values);
      return Response.json({ ok: true });
    } catch (e) {
      // A failed lookup is retried on the next report (fields may have been added).
      board = null;
      console.error(e);
      return text(502, "could not file the report, please try again later");
    }
  },
} satisfies ExportedHandler<Env>;
