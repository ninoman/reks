/** Building blocks the export review and report share: a titled card, facts, notices, file lists. */
import type { ReactNode } from "react";

import { cn } from "../../lib/cn";
import type { DryRunFileGroup, Fact, NoticeGroup } from "../../lib/ipc";
import { Card } from "../ui/Card";

export const list = "m-0 mt-1 list-disc pl-4.5 text-xs [overflow-wrap:anywhere]";

const facts =
  "m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 [&_dt]:text-fg-muted [&_dd]:m-0 [&_dd]:tabular-nums";

export function Section({
  title,
  tone,
  children,
}: {
  title: string;
  tone?: "danger" | "warning" | undefined;
  children: ReactNode;
}) {
  const titleClass =
    tone === "danger" ? "text-danger" : tone === "warning" ? "text-warning" : "text-fg";
  return (
    <section aria-label={title}>
      <Card className="flex flex-col gap-2 px-4 py-3" divided={false}>
        <h3 className={cn("m-0 text-sm font-semibold", titleClass)}>{title}</h3>
        {children}
      </Card>
    </section>
  );
}

export function Facts({ facts: rows }: { facts: Fact[] }) {
  return (
    <dl className={facts}>
      {rows.map((f) => (
        <div key={f.label} className="contents">
          <dt>{f.label}</dt>
          <dd>{f.value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function Notices({ groups }: { groups: NoticeGroup[] }) {
  return (
    <div className="flex flex-col gap-2">
      {groups.map((g) =>
        g.items.length === 0 ? (
          <p key={g.kind} className="m-0 text-sm">
            {g.title}
          </p>
        ) : (
          <details key={g.kind} open={g.items.length <= 3}>
            <summary className="text-sm">{g.title}</summary>
            <ul className={list}>
              {g.items.map((item, i) => (
                // Items of one group can repeat (same message); order is stable.
                // biome-ignore lint/suspicious/noArrayIndexKey: see above
                <li key={i}>{item}</li>
              ))}
            </ul>
          </details>
        ),
      )}
    </div>
  );
}

export function Files({ label, group }: { label: string; group: DryRunFileGroup }) {
  if (group.count === 0) return null;
  return (
    <details>
      <summary className="text-sm">
        {label}: {group.totalText}
      </summary>
      <ul className={cn(list, "font-mono")}>
        {group.files.map((f) => (
          <li key={f.path} title={f.from ?? undefined}>
            {f.path} <span className="text-fg-muted">({f.sizeText})</span>
          </li>
        ))}
        {group.count > group.files.length && (
          <li className="text-fg-muted">…and {group.count - group.files.length} more</li>
        )}
      </ul>
    </details>
  );
}
