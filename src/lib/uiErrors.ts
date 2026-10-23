/**
 * Sends uncaught UI errors to the app log (D33), so a later report's log
 * tail shows them. Logging failures are dropped: there is nowhere else to
 * report them.
 */
import { logUiError } from "./ipc";

export function logError(error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  const stack = error instanceof Error ? (error.stack ?? null) : null;
  logUiError(message, stack).catch(() => {});
}

export function installUiErrorLogging(): void {
  window.addEventListener("error", (e) => logError(e.error ?? e.message));
  window.addEventListener("unhandledrejection", (e) => logError(e.reason));
}
