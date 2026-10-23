import { logFrontendError } from "./tauri-client";

export function describeError(error: unknown): string {
  if (error instanceof Error) return error.stack ?? `${error.name}: ${error.message}`;
  return String(error);
}

/** Forwards uncaught errors and unhandled rejections to the diagnostic log. */
export function installDiagnosticErrorLogging(target: Window = window): void {
  target.addEventListener("error", (event) => {
    const detail = event.error ? describeError(event.error) : event.message;
    logFrontendError(
      `[uncaught] ${detail} (${event.filename}:${event.lineno}:${event.colno})`,
    );
  });
  target.addEventListener("unhandledrejection", (event) => {
    logFrontendError(`[unhandled-rejection] ${describeError(event.reason)}`);
  });
}
