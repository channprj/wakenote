// @vitest-environment jsdom

import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockModels } from "@/lib/app-state";
import type { MeetingDetail, MeetingSummary } from "@/lib/types";

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
  isTauriRuntime: vi.fn(),
  listMeetings: vi.fn(),
  listen: vi.fn(),
  eventHandlers: new Map<
    string,
    (event: { payload: Record<string, unknown> }) => void
  >(),
  meetingDetail: vi.fn(),
  startMeetingTranscription: vi.fn(),
}));

vi.mock("@/lib/tauri-client", () => ({
  cancelMeeting: vi.fn(),
  importMeetingRecording: mocks.importMeetingRecording,
  isTauriRuntime: mocks.isTauriRuntime,
  listMeetings: mocks.listMeetings,
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
  startMeetingTranscription: mocks.startMeetingTranscription,
  startManualMeetingRecording: mocks.startManualMeetingRecording,
  stopManualMeetingRecording: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen,
}));

import { MeetingTranscriptionPanel } from "./MeetingTranscriptionPanel";

beforeEach(() => {
  mocks.isTauriRuntime.mockReset();
  mocks.isTauriRuntime.mockReturnValue(false);
  mocks.eventHandlers.clear();
  mocks.listen.mockReset();
  mocks.listen.mockImplementation(
    async (
      eventName: string,
      handler: (event: { payload: Record<string, unknown> }) => void,
    ) => {
      mocks.eventHandlers.set(eventName, handler);
      return vi.fn();
    },
  );
  mocks.startManualMeetingRecording.mockReset();
  mocks.importMeetingRecording.mockReset();
  mocks.importMeetingRecording.mockResolvedValue(null);
  mocks.meetingDetail.mockReset();
  mocks.listMeetings.mockReset();
  mocks.listMeetings.mockResolvedValue([]);
  mocks.startMeetingTranscription.mockReset();
  mocks.startMeetingTranscription.mockResolvedValue({});
  mocks.startManualMeetingRecording.mockResolvedValue({
    ...mocks.idleRecording,
    state: "recording",
  });
});

afterEach(cleanup);

function meetingSummary(
  overrides: Pick<MeetingSummary, "id" | "title"> & Partial<MeetingSummary>,
): MeetingSummary {
  return {
    source_filename: `${overrides.id}.m4a`,
    status: "recorded",
    duration_ms: 1_800_000,
    created_at: "2026-08-12T01:00:00Z",
    updated_at: "2026-08-12T01:00:00Z",
    progress: {
      segments_total: 0,
      segments_done: 0,
      processed_ms: 0,
      elapsed_ms: 0,
    },
    model_id: "whisper-medium",
    language: "auto",
    error: null,
    capture_warning: null,
    ...overrides,
  };
}

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
    const summary = meetingSummary({
      id: "meeting-imported",
      title: "Imported meeting",
      source_filename: "meeting.m4a",
      duration_ms: 0,
      created_at: "2026-08-10T01:00:00Z",
      updated_at: "2026-08-10T01:00:00Z",
    });
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

    await waitFor(() =>
      expect(mocks.meetingDetail).toHaveBeenCalledWith(summary.id),
    );
    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(screen.getByText("Transcribe saved meeting")).toBeTruthy();
  });

  it("starts every eligible selected meeting with one shared request", async () => {
    const summaries = [
      meetingSummary({
        id: "recorded",
        title: "Recorded sync",
        duration_ms: 3_600_000,
      }),
      meetingSummary({
        id: "failed",
        title: "Failed review",
        status: "failed",
        duration_ms: 3_600_000,
        created_at: "2026-08-12T02:00:00Z",
        updated_at: "2026-08-12T02:00:00Z",
        progress: {
          segments_total: 8,
          segments_done: 3,
          processed_ms: 1_200_000,
          elapsed_ms: 400_000,
        },
        error: "Provider stopped",
      }),
      meetingSummary({
        id: "completed",
        title: "Completed retro",
        status: "completed",
        duration_ms: 3_600_000,
        created_at: "2026-08-12T03:00:00Z",
        updated_at: "2026-08-12T03:00:00Z",
        progress: {
          segments_total: 8,
          segments_done: 8,
          processed_ms: 3_600_000,
          elapsed_ms: 600_000,
        },
        language: "ko",
      }),
    ];
    mocks.listMeetings.mockResolvedValue(summaries);
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
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
      />,
    );

    await userEvent.click(
      await screen.findByRole("checkbox", { name: "Select Recorded sync" }),
    );
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Select Failed review" }),
    );
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Select Completed retro" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Transcribe selected" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start 2 transcriptions" }),
    );

    const request = {
      model_id: "whisper-medium",
      language: "auto",
      streaming_enabled: false,
      speaker_separation_enabled: false,
    };
    await waitFor(() => {
      expect(mocks.startMeetingTranscription).toHaveBeenCalledTimes(2);
    });
    expect(mocks.startMeetingTranscription).toHaveBeenCalledWith(
      "recorded",
      request,
    );
    expect(mocks.startMeetingTranscription).toHaveBeenCalledWith(
      "failed",
      request,
    );
    expect(mocks.startMeetingTranscription).not.toHaveBeenCalledWith(
      "completed",
      request,
    );
  });

  it("keeps only failed meetings selected after a partial bulk start", async () => {
    const summaries = [
      meetingSummary({
        id: "starts",
        title: "Starts successfully",
      }),
      meetingSummary({
        id: "fails",
        title: "Fails to start",
        status: "failed",
        created_at: "2026-08-12T02:00:00Z",
        updated_at: "2026-08-12T02:00:00Z",
        progress: {
          segments_total: 6,
          segments_done: 2,
          processed_ms: 600_000,
          elapsed_ms: 120_000,
        },
        error: "Provider stopped",
      }),
    ];
    mocks.listMeetings.mockResolvedValue(summaries);
    mocks.startMeetingTranscription
      .mockResolvedValueOnce({})
      .mockRejectedValueOnce(new Error("provider unavailable"));
    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={vi.fn().mockResolvedValue(true)}
        models={mockModels()}
        meetingModelId="whisper-medium"
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
      />,
    );

    const starts = await screen.findByRole("checkbox", {
      name: "Select Starts successfully",
    });
    const fails = screen.getByRole("checkbox", {
      name: "Select Fails to start",
    });
    await userEvent.click(starts);
    await userEvent.click(fails);
    await userEvent.click(
      screen.getByRole("button", { name: "Transcribe selected" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start 2 transcriptions" }),
    );

    expect(
      within(screen.getByRole("dialog")).getByText(
        "Could not start 1 of 2 selected meetings.",
      ),
    ).toBeTruthy();
    await waitFor(() => {
      expect(starts.getAttribute("aria-checked")).toBe("false");
      expect(fails.getAttribute("aria-checked")).toBe("true");
    });
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("submits selected meetings in order for the shared worker queue", async () => {
    const summaries = [
      meetingSummary({
        id: "first",
        title: "First queued meeting",
      }),
      meetingSummary({
        id: "second",
        title: "Second queued meeting",
        created_at: "2026-08-12T02:00:00Z",
        updated_at: "2026-08-12T02:00:00Z",
      }),
    ];
    let resolveFirst: ((value: object) => void) | undefined;
    const firstStart = new Promise<object>((resolve) => {
      resolveFirst = resolve;
    });
    mocks.listMeetings.mockResolvedValue(summaries);
    mocks.startMeetingTranscription
      .mockReturnValueOnce(firstStart)
      .mockResolvedValueOnce({});
    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={vi.fn().mockResolvedValue(true)}
        models={mockModels()}
        meetingModelId="whisper-medium"
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
      />,
    );

    await userEvent.click(
      await screen.findByRole("checkbox", {
        name: "Select First queued meeting",
      }),
    );
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Select Second queued meeting" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Transcribe selected" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start 2 transcriptions" }),
    );

    await waitFor(() =>
      expect(mocks.startMeetingTranscription).toHaveBeenCalledTimes(1),
    );
    expect(mocks.startMeetingTranscription).toHaveBeenNthCalledWith(
      1,
      "first",
      expect.any(Object),
    );
    resolveFirst?.({});
    await waitFor(() =>
      expect(mocks.startMeetingTranscription).toHaveBeenCalledTimes(2),
    );
    expect(mocks.startMeetingTranscription).toHaveBeenNthCalledWith(
      2,
      "second",
      expect.any(Object),
    );
  });

  it("removes transient committed text before loading the finished detail", async () => {
    mocks.isTauriRuntime.mockReturnValue(true);
    const summary = meetingSummary({
      id: "meeting-active",
      title: "In-flight planning",
      source_filename: "planning.m4a",
      status: "processing",
      duration_ms: 3_600_000,
      created_at: "2026-08-12T04:00:00Z",
      updated_at: "2026-08-12T04:10:00Z",
      progress: {
        phase: "transcribing",
        segments_total: 10,
        segments_done: 2,
        processed_ms: 720_000,
        elapsed_ms: 300_000,
      },
      language: "ko",
    });
    const processingDetail: MeetingDetail = {
      record: {
        ...summary,
        audio_file: "planning.m4a",
        audio_format: "m4a",
        app_version: "0.260812.0",
        segments: [],
        transcription_request: null,
        speaker_turns: [],
        api_audio_duration_ms: 0,
        api_cost_microusd: 0,
        api_request_count: 0,
        api_unpriced_request_count: 0,
      },
      transcript: "Persisted opening.",
      audio_path: "/tmp/planning.m4a",
    };
    let resolveReload: ((detail: MeetingDetail) => void) | undefined;
    const reload = new Promise<MeetingDetail>((resolve) => {
      resolveReload = resolve;
    });
    mocks.listMeetings.mockResolvedValue([summary]);
    mocks.meetingDetail
      .mockResolvedValueOnce(processingDetail)
      .mockReturnValueOnce(reload);
    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={vi.fn().mockResolvedValue(true)}
        models={mockModels()}
        meetingModelId="whisper-medium"
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
      />,
    );

    await waitFor(() =>
      expect(mocks.eventHandlers.has("meeting-finished")).toBe(true),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "In-flight planning" }),
    );
    mocks.eventHandlers.get("meeting-segment-committed")?.({
      payload: {
        id: summary.id,
        index: 2,
        start_ms: 720_000,
        end_ms: 1_080_000,
        text: "Transient committed segment.",
      },
    });
    expect(
      await screen.findByText("Transient committed segment."),
    ).toBeTruthy();

    mocks.eventHandlers.get("meeting-finished")?.({
      payload: { id: summary.id, status: "completed", error: null },
    });
    await waitFor(() =>
      expect(screen.queryByText("Transient committed segment.")).toBeNull(),
    );
    resolveReload?.({
      ...processingDetail,
      record: { ...processingDetail.record, status: "completed" },
      transcript: "Canonical final transcript.",
    });
    expect(await screen.findByText("Canonical final transcript.")).toBeTruthy();
  });

  it("rebases transient text onto the canonical transcript when detail opens", async () => {
    mocks.isTauriRuntime.mockReturnValue(true);
    const summary = meetingSummary({
      id: "meeting-rebased",
      title: "Rebased transcript",
      source_filename: "rebased.m4a",
      status: "processing",
      duration_ms: 1_800_000,
      created_at: "2026-08-12T05:00:00Z",
      updated_at: "2026-08-12T05:05:00Z",
      progress: {
        phase: "transcribing",
        segments_total: 6,
        segments_done: 1,
        processed_ms: 300_000,
        elapsed_ms: 60_000,
      },
      language: "en",
    });
    mocks.listMeetings.mockResolvedValue([summary]);
    mocks.meetingDetail.mockResolvedValue({
      record: {
        ...summary,
        audio_file: "rebased.m4a",
        audio_format: "m4a",
        app_version: "0.260812.0",
        segments: [],
        transcription_request: null,
        speaker_turns: [],
        api_audio_duration_ms: 0,
        api_cost_microusd: 0,
        api_request_count: 0,
        api_unpriced_request_count: 0,
      },
      transcript: "Committed once.",
      audio_path: "/tmp/rebased.m4a",
    } satisfies MeetingDetail);
    render(
      <MeetingTranscriptionPanel
        onPermissionRequired={vi.fn().mockResolvedValue(true)}
        models={mockModels()}
        meetingModelId="whisper-medium"
        selectedModelId="whisper-medium"
        onMeetingModelChange={vi.fn()}
      />,
    );

    await waitFor(() =>
      expect(mocks.eventHandlers.has("meeting-segment-committed")).toBe(true),
    );
    mocks.eventHandlers.get("meeting-segment-committed")?.({
      payload: {
        id: summary.id,
        index: 0,
        start_ms: 0,
        end_ms: 300_000,
        text: "Committed once.",
      },
    });
    await userEvent.click(
      await screen.findByRole("button", { name: "Rebased transcript" }),
    );

    await screen.findByRole("log", { name: "Live transcript" });
    expect(screen.getAllByText("Committed once.")).toHaveLength(1);
    expect(
      screen.getByText("Waiting for the first processed segment."),
    ).toBeTruthy();
  });
});
