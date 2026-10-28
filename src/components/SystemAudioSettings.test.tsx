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
  it("gates Resume capture on the Screen Recording preflight", async () => {
    for (const granted of [false, true]) {
      cleanup();
      mocks.startSourceCapture.mockReset();
      if (granted) {
        mocks.startSourceCapture.mockResolvedValue(mockSnapshot());
      }
      const user = userEvent.setup();
      const onPermissionRequired = vi.fn().mockResolvedValue(granted);
      render(
        <SystemAudioSettings
          settings={mockSnapshot().settings}
          onPatch={() => {}}
          onPermissionRequired={onPermissionRequired}
        />,
      );

      await user.click(await screen.findByRole("button", { name: "Resume capture" }));

      const label = `Screen Recording granted=${granted}`;
      expect(onPermissionRequired, label).toHaveBeenCalledOnce();
      if (granted) {
        await waitFor(() =>
          expect(mocks.startSourceCapture, label).toHaveBeenCalledWith("meet"),
        );
      } else {
        expect(mocks.startSourceCapture, label).not.toHaveBeenCalled();
      }
    }
  });
});
