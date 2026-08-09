// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  idleRecording: {
    generation: 0,
    state: "off" as const,
    meeting_id: null,
    started_at: null,
    elapsed_ms: 0,
    remaining_ms: 18_000_000,
    inputs: ["Microphone", "System Audio"],
    stop_reason: null,
    error: null,
  },
  startManualMeetingRecording: vi.fn(),
}));

vi.mock("@/lib/tauri-client", () => ({
  cancelMeeting: vi.fn(),
  importAndStartMeeting: vi.fn().mockResolvedValue(null),
  isTauriRuntime: () => false,
  listMeetings: vi.fn().mockResolvedValue([]),
  listTranscriptionModels: vi.fn().mockResolvedValue([]),
  loadListVisibility: vi.fn().mockResolvedValue({
    meetings: [],
    transcripts: [],
    report_runs: [],
    legacy_reports: [],
  }),
  loadManualMeetingRecordingStatus: vi
    .fn()
    .mockResolvedValue(mocks.idleRecording),
  meetingDetail: vi.fn(),
  openTranscriptFolder: vi.fn(),
  resumeMeeting: vi.fn(),
  setListVisibility: vi.fn(),
  startMeetingTranscription: vi.fn(),
  startManualMeetingRecording: mocks.startManualMeetingRecording,
  stopManualMeetingRecording: vi.fn(),
}));

import { MeetingTranscriptionPanel } from "./MeetingTranscriptionPanel";

beforeEach(() => {
  mocks.startManualMeetingRecording.mockReset();
  mocks.startManualMeetingRecording.mockResolvedValue({
    ...mocks.idleRecording,
    state: "recording",
  });
});

afterEach(cleanup);

describe("MeetingTranscriptionPanel permission preflight", () => {
  it("does not start a manual meeting when required permissions are missing", async () => {
    const user = userEvent.setup();
    const onPermissionRequired = vi.fn().mockResolvedValue(false);
    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={onPermissionRequired}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Start meeting recording" }),
    );

    expect(onPermissionRequired).toHaveBeenCalledWith("manual_meeting");
    expect(mocks.startManualMeetingRecording).not.toHaveBeenCalled();
  });

  it("starts a manual meeting after the permission preflight succeeds", async () => {
    const user = userEvent.setup();
    const onPermissionRequired = vi.fn().mockResolvedValue(true);
    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={onPermissionRequired}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Start meeting recording" }),
    );

    await waitFor(() =>
      expect(mocks.startManualMeetingRecording).toHaveBeenCalledOnce(),
    );
  });
});
