// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockModels } from "@/lib/app-state";

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
  importMeetingRecording: vi.fn(),
  meetingDetail: vi.fn(),
}));

vi.mock("@/lib/tauri-client", () => ({
  cancelMeeting: vi.fn(),
  importMeetingRecording: mocks.importMeetingRecording,
  isTauriRuntime: () => false,
  listMeetings: vi.fn().mockResolvedValue([]),
  loadListVisibility: vi.fn().mockResolvedValue({
    meetings: [],
    transcripts: [],
    report_runs: [],
    legacy_reports: [],
  }),
  loadManualMeetingRecordingStatus: vi
    .fn()
    .mockResolvedValue(mocks.idleRecording),
  meetingDetail: mocks.meetingDetail,
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
  mocks.importMeetingRecording.mockReset();
  mocks.importMeetingRecording.mockResolvedValue(null);
  mocks.meetingDetail.mockReset();
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
        models={[]}
        meetingModelId=""
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
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
        models={[]}
        meetingModelId=""
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Start meeting recording" }),
    );

    await waitFor(() =>
      expect(mocks.startManualMeetingRecording).toHaveBeenCalledOnce(),
    );
  });

  it("opens configuration without starting transcription after import", async () => {
    const summary = {
      id: "meeting-imported",
      title: "Imported meeting",
      source_filename: "meeting.m4a",
      status: "recorded" as const,
      duration_ms: 0,
      created_at: "2026-08-10T01:00:00Z",
      updated_at: "2026-08-10T01:00:00Z",
      progress: {
        segments_total: 0,
        segments_done: 0,
        processed_ms: 0,
        elapsed_ms: 0,
      },
      model_id: "whisper-medium",
      language: "auto" as const,
      error: null,
    };
    mocks.importMeetingRecording.mockResolvedValue(summary);
    mocks.meetingDetail.mockResolvedValue({
      record: {
        ...summary,
        audio_file: "audio.m4a",
        audio_format: "m4a",
        app_version: "0.260810.0",
        segments: [],
        transcription_request: null,
        speaker_turns: [],
        api_audio_duration_ms: 0,
        api_cost_microusd: 0,
        api_request_count: 0,
        api_unpriced_request_count: 0,
      },
      transcript: "",
      audio_path: "/tmp/audio.m4a",
    });
    const models = mockModels().map((model) =>
      model.id === "whisper-medium"
        ? { ...model, status: "ready" as const }
        : model,
    );

    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={vi.fn().mockResolvedValue(true)}
        models={models}
        meetingModelId="whisper-medium"
        selectedModelId="soniox-realtime-v5"
        onMeetingModelChange={vi.fn()}
      />,
    );

    await userEvent.click(
      await screen.findByRole("button", { name: "Import meeting audio" }),
    );

    await waitFor(() => expect(mocks.meetingDetail).toHaveBeenCalledWith(summary.id));
    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(screen.getByText("Transcribe saved meeting")).toBeTruthy();
  });
});
