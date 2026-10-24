/**
 * A drive's check report in the main area ("Check stick" in the sidebar's
 * Devices section): a summary, the library files with the players that read
 * each (UNVERIFIED), and the findings grouped by section, each group's
 * tracks or playlists folded away. All wording comes from Rust
 * (`reks_export_rb::check`); nothing here changes the stick.
 */
import type {
  CheckFindingGroup,
  CheckFormatRow,
  CheckSeverity,
  DeviceInfo,
  StickReport,
} from "../lib/ipc";
import { useDeviceBrowser } from "../store/deviceBrowser";
import { deviceName, useDevices } from "../store/devices";
import { useStickCheck } from "../store/stickCheck";
import { ErrorMessage } from "./ErrorMessage";
import { CloseIcon } from "./icons";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Caption } from "./ui/Caption";
import { Card } from "./ui/Card";
import { IconButton } from "./ui/IconButton";
import { ScrollArea } from "./ui/ScrollArea";

const SEVERITY: Record<CheckSeverity, { label: string; variant: "danger" | "warning" | "muted" }> =
  {
    problem: { label: "Problem", variant: "danger" },
    warning: { label: "Warning", variant: "warning" },
    note: { label: "Note", variant: "muted" },
  };

const STATUS: Record<
  StickReport["summary"]["status"],
  { label: string; variant: "danger" | "warning" | "success" }
> = {
  ok: { label: "OK", variant: "success" },
  warnings: { label: "Warnings", variant: "warning" },
  problems: { label: "Problems", variant: "danger" },
};

export function StickCheckView() {
  const mountPoint = useDeviceBrowser((s) => s.checking);
  const load = useStickCheck((s) => (mountPoint === null ? undefined : s.loads[mountPoint]));
  const device = useDevices((s) => s.devices.find((d) => d.mountPoint === mountPoint));
  if (mountPoint === null) return null;
  const name = device === undefined ? mountPoint : deviceName(device);
  const busy = load === undefined || load.state === "loading";

  return (
    <section className="flex min-h-0 flex-1 flex-col" aria-label={`Check ${name}`}>
      <header className="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-b border-border bg-bg-subtle px-3 py-2">
        <h2 className="m-0 text-md font-semibold">Check {name}</h2>
        <span className="text-sm text-fg-muted">Read-only: nothing on the stick changes.</span>
        <span className="ml-auto flex items-center gap-1">
          {device !== undefined && <CheckAgain device={device} busy={busy} />}
          <IconButton
            size="sm"
            aria-label="Close the check"
            title="Close"
            onClick={() => useDeviceBrowser.getState().close()}
          >
            <CloseIcon />
          </IconButton>
        </span>
      </header>
      {busy ? (
        <p className="m-0 flex flex-1 items-center justify-center text-fg-muted" role="status">
          Checking every track on the stick…
        </p>
      ) : load.state === "error" ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2">
          <ErrorMessage message={load.message} />
          {device !== undefined && <CheckAgain device={device} busy={false} label="Try again" />}
        </div>
      ) : (
        <Report report={load.report} />
      )}
    </section>
  );
}

function CheckAgain({
  device,
  busy,
  label = "Check again",
}: {
  device: DeviceInfo;
  busy: boolean;
  label?: string;
}) {
  return (
    <Button size="sm" disabled={busy} onClick={() => void useStickCheck.getState().run(device)}>
      {label}
    </Button>
  );
}

function Report({ report: r }: { report: StickReport }) {
  const status = STATUS[r.summary.status];
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="flex max-w-4xl flex-col gap-4 p-3">
        <Card variant="surface" className="flex flex-col gap-1 px-3 py-2.5" divided={false}>
          <p className="m-0 flex items-center gap-2" role="status">
            <Badge variant={status.variant} size="md">
              {status.label}
            </Badge>
            <span className="text-md font-semibold">{r.summary.headline}</span>
            {r.summary.notes > 0 && (
              <span className="text-sm text-fg-muted">
                {r.summary.notes} note{r.summary.notes === 1 ? "" : "s"}
              </span>
            )}
          </p>
          <p className="m-0 text-sm text-fg-muted">
            {r.tracksChecked} track{r.tracksChecked === 1 ? "" : "s"} checked · {r.filesystem.text}
          </p>
          <p className="m-0 text-sm text-fg-muted">{r.targetText}</p>
        </Card>

        <Formats rows={r.formats} source={r.playersSource} />

        {r.sections.length === 0 ? (
          <p className="m-0 text-fg-muted">Nothing to report: every check passed.</p>
        ) : (
          r.sections.map((s) => (
            <section key={s.section} className="flex flex-col gap-1.5" aria-label={s.title}>
              <Caption asChild>
                <h3 className="m-0">{s.title}</h3>
              </Caption>
              <Card variant="subtle">
                {s.groups.map((g) => (
                  <Group key={`${g.kind}-${g.severity}-${g.title}`} group={g} />
                ))}
              </Card>
            </section>
          ))
        )}
      </div>
    </ScrollArea>
  );
}

function Formats({ rows, source }: { rows: CheckFormatRow[]; source: string }) {
  return (
    <section className="flex flex-col gap-1.5" aria-label="Library files">
      <Caption asChild>
        <h3 className="m-0">Library files</h3>
      </Caption>
      <Card variant="subtle">
        {rows.map((f) => (
          <div key={f.format} className="flex flex-col gap-0.5 px-3 py-2">
            <p className="m-0 flex flex-wrap items-baseline gap-x-2">
              <span className="font-semibold">{f.name}</span>
              <code className="text-xs text-fg-subtle">{f.file}</code>
              <span className={f.present ? "text-sm" : "text-sm text-fg-subtle"}>{f.status}</span>
            </p>
            {f.players.length > 0 && (
              <p className="m-0 flex flex-wrap items-center gap-x-1.5 text-sm text-fg-muted">
                Read by {f.players.join(", ")}
                {!f.playersVerified && <Badge variant="muted">UNVERIFIED</Badge>}
              </p>
            )}
            <p className="m-0 text-xs text-fg-subtle">{f.note}</p>
          </div>
        ))}
      </Card>
      <p className="m-0 text-xs text-fg-subtle">{source}</p>
    </section>
  );
}

function Group({ group: g }: { group: CheckFindingGroup }) {
  const sev = SEVERITY[g.severity];
  return (
    <div className="flex flex-col gap-1 px-3 py-2">
      <p className="m-0 flex items-baseline gap-2">
        <Badge variant={sev.variant}>{sev.label}</Badge>
        <span className="font-medium">{g.title}</span>
      </p>
      <p className="m-0 text-sm text-fg-muted">{g.detail}</p>
      {g.items.length > 0 && (
        <details className="text-sm">
          <summary className="cursor-default text-fg-muted">
            Show the list ({g.items.length})
          </summary>
          <ul className="m-0 mt-1 flex list-none flex-col gap-0.5 p-0">
            {g.items.map((item, i) => (
              <li
                // A report never reorders; items may repeat (two damaged rows).
                // biome-ignore lint/suspicious/noArrayIndexKey: see above
                key={i}
                className="select-text"
                title={item.path ?? undefined}
              >
                {item.subject}
                {item.detail !== null && <span className="text-fg-muted">: {item.detail}</span>}
                {item.path !== null && item.path !== item.subject && (
                  <code className="ml-1.5 text-xs text-fg-subtle">{item.path}</code>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
