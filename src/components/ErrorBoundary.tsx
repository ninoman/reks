/**
 * Catches a render error: logs it (D34) and offers a bug report instead of
 * a blank window. With `region` it replaces only that part of the screen
 * and offers "Try again", so the rest keeps working (and the engine keeps
 * its state); without, it is the last-resort full screen with Reload.
 */
import { Component, type ErrorInfo, type ReactNode } from "react";

import { logError } from "../lib/uiErrors";
import { useReport } from "../store/report";
import { ReportDialog } from "./ReportDialog";
import { Button } from "./ui/Button";

interface State {
  error: Error | null;
}

interface Props {
  children: ReactNode;
  /** Names the part of the screen this guards, e.g. "Deck". */
  region?: string;
}

export class ErrorBoundary extends Component<Props, State> {
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
    const { region } = this.props;
    if (error === null) return this.props.children;
    if (region !== undefined) {
      return (
        <div
          role="alert"
          className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-4 text-fg"
        >
          <p className="m-0 text-sm font-semibold">{region} hit a problem</p>
          <p className="m-0 max-w-[420px] text-center text-xs text-fg-muted select-text">
            {error.message}
          </p>
          <div className="flex gap-2">
            <Button onClick={() => this.setState({ error: null })}>Try again</Button>
            <Button
              variant="primary"
              onClick={() =>
                void useReport
                  .getState()
                  .openReport({ kind: "bug", context: `${region}: ${error.message}` })
              }
            >
              Report this
            </Button>
          </div>
        </div>
      );
    }
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
