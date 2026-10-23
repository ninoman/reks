/**
 * Last-resort screen when rendering throws: logs the error (D34) and offers
 * a bug report about it or a reload, instead of a blank window.
 */
import { Component, type ErrorInfo, type ReactNode } from "react";

import { logError } from "../lib/uiErrors";
import { useReport } from "../store/report";
import { ReportDialog } from "./ReportDialog";
import { Button } from "./ui/Button";

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    logError(
      Object.assign(new Error(error.message), { stack: `${error.stack}${info.componentStack}` }),
    );
  }

  render() {
    const { error } = this.state;
    if (error === null) return this.props.children;
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 bg-bg p-6 text-fg">
        <p className="m-0 text-md font-semibold">Something went wrong</p>
        <p className="m-0 max-w-[480px] text-center text-sm text-fg-muted select-text">
          {error.message}
        </p>
        <div className="flex gap-2">
          <Button
            variant="primary"
            onClick={() =>
              void useReport.getState().openReport({ kind: "bug", context: error.message })
            }
          >
            Report this
          </Button>
          <Button onClick={() => window.location.reload()}>Reload</Button>
        </div>
        <ReportDialog />
      </div>
    );
  }
}
