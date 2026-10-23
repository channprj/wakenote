import { beforeEach, describe, expect, it, vi } from "vitest";

const { logFrontendError } = vi.hoisted(() => ({ logFrontendError: vi.fn() }));
vi.mock("./tauri-client", () => ({ logFrontendError }));

import { describeError, installDiagnosticErrorLogging } from "./diagnostic-log";

describe("diagnostic error logging", () => {
  beforeEach(() => logFrontendError.mockReset());

  it("prefers the stack trace and stringifies non-errors", () => {
    const error = new Error("boom");
    error.stack = "Error: boom\n    at render (App.tsx:1:2)";
    expect(describeError(error)).toBe(error.stack);
    expect(describeError("plain reason")).toBe("plain reason");
  });

  it("forwards uncaught errors and unhandled rejections", () => {
    const target = new EventTarget() as unknown as Window;
    installDiagnosticErrorLogging(target);

    const error = new Error("render failed");
    error.stack = "Error: render failed";
    target.dispatchEvent(
      Object.assign(new Event("error"), {
        error,
        message: "render failed",
        filename: "App.tsx",
        lineno: 3,
        colno: 9,
      }),
    );
    target.dispatchEvent(
      Object.assign(new Event("unhandledrejection"), { reason: "network down" }),
    );

    expect(logFrontendError).toHaveBeenNthCalledWith(
      1,
      "[uncaught] Error: render failed (App.tsx:3:9)",
    );
    expect(logFrontendError).toHaveBeenNthCalledWith(
      2,
      "[unhandled-rejection] network down",
    );
  });
});
