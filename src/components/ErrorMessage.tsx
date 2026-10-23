/**
 * An error shown in place, with a "Report" link that opens a bug report
 * carrying the message (D33). `ReportErrorButton` is the link alone, for
 * error blocks that lay out their own text and buttons.
 */
import { cn } from "../lib/cn";
import { useReport } from "../store/report";
import { Button } from "./ui/Button";

export function ReportErrorButton({ message }: { message: string }) {
  return (
    <Button
      variant="link"
      size="xs"
      className="h-auto align-baseline"
      onClick={() => void useReport.getState().openReport({ kind: "bug", context: message })}
    >
      Report
    </Button>
  );
}

export function ErrorMessage({
  message,
  as: Tag = "div",
  className,
}: {
  message: string;
  as?: "div" | "p" | "span";
  className?: string;
}) {
  return (
    <Tag className={cn("text-danger select-text", className)}>
      {/* Only the message is announced; the link stays out of the alert. */}
      <span role="alert">{message}</span> <ReportErrorButton message={message} />
    </Tag>
  );
}
