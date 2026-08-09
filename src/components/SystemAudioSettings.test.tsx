// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "@/lib/app-state";

const mocks = vi.hoisted(() => ({
  startSourceCapture: vi.fn(),
}));

vi.mock("@/lib/tauri-client", () => ({
  loadRecognizedSources: vi.fn().mockResolvedValue([]),
  loadSourceCaptureStatus: vi.fn().mockResolvedValue({
    detected: {
      source_id: "meet",
      label: "Google Meet",
      app_name: "Chrome",
    },
    capturing: false,
  }),
  startSourceCapture: mocks.startSourceCapture,
  stopSourceCapture: vi.fn(),
}));

import { SystemAudioSettings } from "./SystemAudioSettings";

beforeEach(() => {
  mocks.startSourceCapture.mockReset();
});

afterEach(cleanup);

describe("SystemAudioSettings permission preflight", () => {
  it("does not resume a detected source when Screen Recording is missing", async () => {
    const user = userEvent.setup();
    const onPermissionRequired = vi.fn().mockResolvedValue(false);
    render(
      <SystemAudioSettings
        settings={mockSnapshot().settings}
        onPatch={() => {}}
        onPermissionRequired={onPermissionRequired}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Resume capture" }));

    expect(onPermissionRequired).toHaveBeenCalledOnce();
    expect(mocks.startSourceCapture).not.toHaveBeenCalled();
  });

  it("resumes a detected source after permission is granted", async () => {
    const user = userEvent.setup();
    const onPermissionRequired = vi.fn().mockResolvedValue(true);
    mocks.startSourceCapture.mockResolvedValue(mockSnapshot());
    render(
      <SystemAudioSettings
        settings={mockSnapshot().settings}
        onPatch={() => {}}
        onPermissionRequired={onPermissionRequired}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Resume capture" }));

    await waitFor(() =>
      expect(mocks.startSourceCapture).toHaveBeenCalledWith("meet"),
    );
  });
});
