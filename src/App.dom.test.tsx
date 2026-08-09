// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "./lib/app-state";
import type { AppSnapshot } from "./lib/types";

const mocks = vi.hoisted(() => ({
  loadSnapshot: vi.fn<() => Promise<AppSnapshot>>(),
  loadRecentTranscripts: vi.fn().mockResolvedValue([]),
  loadDictationState: vi.fn().mockResolvedValue({ state: "idle", error: null }),
  saveSettingsPatch: vi.fn(),
  startLiveCapture: vi.fn(),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("./lib/tauri-client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/tauri-client")>();
  return {
    ...actual,
    loadSnapshot: mocks.loadSnapshot,
    loadRecentTranscripts: mocks.loadRecentTranscripts,
    loadDictationState: mocks.loadDictationState,
    saveSettingsPatch: mocks.saveSettingsPatch,
    startLiveCapture: mocks.startLiveCapture,
  };
});

vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

import App from "./App";

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

globalThis.ResizeObserver = TestResizeObserver as typeof ResizeObserver;

Object.defineProperties(HTMLElement.prototype, {
  hasPointerCapture: {
    configurable: true,
    value: () => false,
  },
  setPointerCapture: {
    configurable: true,
    value: () => {},
  },
  releasePointerCapture: {
    configurable: true,
    value: () => {},
  },
});

function nativeSnapshot(
  marker: boolean,
  missing: "accessibility" | "microphone" | "screen_recording" | null,
) {
  const snapshot = mockSnapshot();
  snapshot.settings.permission_onboarding_seen = marker;
  if (missing) {
    snapshot.permissions[missing] = {
      ...snapshot.permissions[missing],
      status: "denied",
      label: "Needs access",
    };
  }
  return snapshot;
}

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: () => {},
  });
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  mocks.loadRecentTranscripts.mockResolvedValue([]);
  mocks.loadDictationState.mockResolvedValue({ state: "idle", error: null });
  mocks.listen.mockResolvedValue(() => {});
  mocks.loadSnapshot.mockReset();
  mocks.saveSettingsPatch.mockReset();
  mocks.startLiveCapture.mockReset();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

describe("permission onboarding", () => {
  it("routes a fresh installation with a missing permission to Settings Audio once", async () => {
    const first = nativeSnapshot(false, "microphone");
    const persisted = structuredClone(first);
    persisted.settings.permission_onboarding_seen = true;
    mocks.loadSnapshot.mockResolvedValue(first);
    mocks.saveSettingsPatch.mockResolvedValue(persisted);

    render(<App />);

    expect(
      (await screen.findByRole("tab", { name: "Audio" })).getAttribute(
        "aria-selected",
      ),
    ).toBe("true");
    expect(mocks.saveSettingsPatch).toHaveBeenCalledTimes(1);
    expect(mocks.saveSettingsPatch).toHaveBeenCalledWith({
      permission_onboarding_seen: true,
    });
    expect(mocks.startLiveCapture).not.toHaveBeenCalled();
  });

  it("persists the fresh-install check without leaving Capture when all permissions are granted", async () => {
    const first = nativeSnapshot(false, null);
    const persisted = structuredClone(first);
    persisted.settings.permission_onboarding_seen = true;
    mocks.loadSnapshot.mockResolvedValue(first);
    mocks.saveSettingsPatch.mockResolvedValue(persisted);

    render(<App />);

    await waitFor(() => expect(mocks.saveSettingsPatch).toHaveBeenCalledOnce());
    expect(screen.getByText("Capture", { selector: "h1" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Audio" })).toBeNull();
  });

  it("does not route or persist an existing installation", async () => {
    mocks.loadSnapshot.mockResolvedValue(nativeSnapshot(true, "microphone"));

    render(<App />);

    await waitFor(() => expect(mocks.loadSnapshot).toHaveBeenCalled());
    expect(screen.getByText("Capture", { selector: "h1" })).toBeTruthy();
    expect(mocks.saveSettingsPatch).not.toHaveBeenCalled();
  });

  it("does not loop after a marker save fails in the same session", async () => {
    const user = userEvent.setup();
    mocks.loadSnapshot.mockResolvedValue(nativeSnapshot(false, "microphone"));
    mocks.saveSettingsPatch.mockRejectedValue(new Error("save failed"));

    render(<App />);

    expect(await screen.findByText("save failed")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Capture" }));
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(mocks.loadSnapshot).toHaveBeenCalledTimes(2));

    expect(screen.getByText("Capture", { selector: "h1" })).toBeTruthy();
    expect(mocks.saveSettingsPatch).toHaveBeenCalledTimes(1);
  });
});
