// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type {
  MeetingDetail,
  MeetingProgressPayload,
  MeetingSummary,
} from "@/lib/types";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { mockModels } from "@/lib/app-state";
import {
  MeetingTranscriptionView,
  type MeetingTranscriptionViewProps,
} from "./MeetingTranscriptionView";

afterEach(cleanup);

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
  scrollIntoView: {
    configurable: true,
    value: () => {},
  },
});

function meeting(overrides: Partial<MeetingSummary> = {}): MeetingSummary {
  return {
    id: "meeting-1",
    title: "Weekly product review",
    source_filename: "weekly-review.m4a",
    status: "completed",
    duration_ms: 7_200_000,
    created_at: "2026-07-15T01:00:00Z",
    updated_at: "2026-07-15T03:00:00Z",
    progress: {
      segments_total: 12,
      segments_done: 12,
      processed_ms: 7_200_000,
      elapsed_ms: 2_000_000,
    },
    model_id: "whisper-medium",
    language: "ko",
    error: null,
    capture_warning: null,
    ...overrides,
  };
}

function detail(overrides: Partial<MeetingSummary> = {}): MeetingDetail {
  const record = meeting(overrides);
  return {
    record: {
      ...record,
      audio_file: "weekly-review.m4a",
      audio_format: "m4a",
      app_version: "0.260715.0",
      segments: [],
      transcription_request: null,
      speaker_turns: [],
      api_audio_duration_ms: 0,
      api_cost_microusd: 0,
      api_request_count: 0,
      api_unpriced_request_count: 0,
    },
    transcript: "첫 번째 문장입니다.\n두 번째 문장입니다.",
    audio_path: "/tmp/weekly-review.m4a",
  };
}

const liveProgress: MeetingProgressPayload = {
  id: "meeting-active",
  status: "processing",
  phase: "transcribing",
  segments_total: 12,
  segments_done: 2,
  processed_ms: 1_200_000,
  duration_ms: 7_200_000,
  elapsed_ms: 300_000,
  eta_ms: 900_000,
};

function props(
  overrides: Partial<MeetingTranscriptionViewProps> = {},
): MeetingTranscriptionViewProps {
  return {
    active: [],
    past: [],
    selected: null,
    progressById: {},
    liveTextById: {},
    busy: false,
    manualRecording: {
      generation: 0,
      state: "off",
      meeting_id: null,
      started_at: null,
      elapsed_ms: 0,
      remaining_ms: 18_000_000,
      inputs: ["Microphone", "System Audio"],
      stop_reason: null,
      error: null,
    },
    manualRecordingBusy: false,
    models: mockModels(),
    meetingModelId: "",
    selectedModelId: "whisper-medium",
    configurationInitiallyOpen: false,
    transcriptionBusy: false,
    error: null,
    visibilityMode: "visible",
    visibleCount: 0,
    hiddenCount: 0,
    selectedMeetingIds: [],
    visibilityMutating: false,
    visibilityStatus: "",
    onImport: vi.fn(),
    onMeetingModelChange: vi.fn(),
    onStartManualRecording: vi.fn(),
    onStopManualRecording: vi.fn(),
    onStartTranscription: vi.fn(),
    onStartBulkTranscription: vi.fn(),
    onOpen: vi.fn(),
    onBack: vi.fn(),
    onCancel: vi.fn(),
    onResume: vi.fn(),
    onCopy: vi.fn(),
    onOpenFolder: vi.fn(),
    onVisibilityModeChange: vi.fn(),
    onMeetingSelectionChange: vi.fn(),
    onSelectAllMeetings: vi.fn(),
    onClearMeetingSelection: vi.fn(),
    onApplyMeetingSelection: vi.fn(),
    onSetMeetingHidden: vi.fn(),
    ...overrides,
  };
}

describe("MeetingTranscriptionView", () => {
  it("shows a non-fatal partial-audio warning on a saved meeting", () => {
    render(
      <MeetingTranscriptionView
        {...props({
          selected: detail({
            status: "recorded",
            capture_warning: "Microphone audio was unavailable",
          }),
        })}
      />,
    );

    expect(screen.getByText("Partial audio")).toBeTruthy();
    expect(screen.getByText("Microphone audio was unavailable")).toBeTruthy();
    expect(screen.queryByText("Meeting transcription interrupted")).toBeNull();
  });

  it("shows runnable realtime models in the dedicated meeting selector", async () => {
    const onMeetingModelChange = vi.fn();
    const models = mockModels().map((model) =>
      model.id === "whisper-medium"
        ? { ...model, status: "ready" as const }
        : model,
    );
    render(
      <MeetingTranscriptionView {...props({ models, onMeetingModelChange })} />,
    );

    const selector = screen.getByRole("combobox", {
      name: "Default meeting transcription model",
    });
    expect(selector).toBeTruthy();
    await userEvent.click(selector);
    expect(
      screen.getByRole("option", { name: /Soniox.*Real-time/ }),
    ).toBeTruthy();
    await userEvent.click(
      screen.getByRole("option", { name: "Soniox · Async V5" }),
    );
    expect(onMeetingModelChange).toHaveBeenCalledWith("soniox-async-v5");
  });

  it("opens configuration immediately after an imported meeting is selected", () => {
    render(
      <MeetingTranscriptionView
        {...props({
          selected: detail({ status: "recorded" }),
          configurationInitiallyOpen: true,
          models: mockModels().map((model) =>
            model.id === "whisper-medium"
              ? { ...model, status: "ready" as const }
              : model,
          ),
        })}
      />,
    );

    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByText("Transcribe saved meeting")).toBeTruthy();
  });

  it("starts and stops a five-hour microphone plus system meeting recording", async () => {
    const onStartManualRecording = vi.fn();
    const onStopManualRecording = vi.fn();
    const { rerender } = render(
      <MeetingTranscriptionView
        {...props({
          manualRecording: {
            generation: 1,
            state: "off",
            meeting_id: null,
            started_at: null,
            elapsed_ms: 0,
            remaining_ms: 18_000_000,
            inputs: ["Microphone", "System Audio"],
            stop_reason: null,
            error: null,
          },
          onStartManualRecording,
          onStopManualRecording,
        })}
      />,
    );

    expect(screen.getByText("Meeting Mode")).toBeTruthy();
    expect(screen.getByText("Microphone + System Audio")).toBeTruthy();
    expect(screen.getByText("5:00:00 maximum")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Start meeting recording" }),
    );
    expect(onStartManualRecording).toHaveBeenCalledOnce();

    rerender(
      <MeetingTranscriptionView
        {...props({
          manualRecording: {
            generation: 1,
            state: "recording",
            meeting_id: "meeting-live",
            started_at: "2026-08-03T01:00:00Z",
            elapsed_ms: 3_723_000,
            remaining_ms: 14_277_000,
            inputs: ["Microphone", "System Audio"],
            stop_reason: null,
            error: null,
          },
          onStartManualRecording,
          onStopManualRecording,
        })}
      />,
    );

    expect(screen.getByText("1:02:03 elapsed")).toBeTruthy();
    expect(screen.getByText("3:57:57 remaining")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Stop and save meeting recording" }),
    );
    expect(onStopManualRecording).toHaveBeenCalledOnce();
  });

  it("renders active segment progress, remaining time, and live text", () => {
    const active = meeting({ id: "meeting-active", status: "processing" });
    const markup = renderToStaticMarkup(
      <MeetingTranscriptionView
        {...props({
          active: [active],
          progressById: { [active.id]: liveProgress },
          liveTextById: { [active.id]: "현재 회의 내용을 전사하고 있습니다." },
        })}
      />,
    );

    expect(markup).toContain("Segment 2/12");
    expect(markup).toContain("Transcribing");
    expect(markup).toContain("Remaining");
    expect(markup).toContain("현재 회의 내용을 전사");
  });

  it("truncates long past titles while exposing the complete title", () => {
    const longTitle =
      "아주 긴 회의 제목이 목록 너비를 넘어가더라도 전체 제목을 확인할 수 있어야 합니다";
    const markup = renderToStaticMarkup(
      <MeetingTranscriptionView
        {...props({ past: [meeting({ title: longTitle })] })}
      />,
    );

    expect(markup).toContain(`title="${longTitle}"`);
    expect(markup).toContain('class="meeting-row__title"');
  });

  it("keeps long active and past meeting content available without truncate classes", () => {
    const active = meeting({
      id: "meeting-active",
      status: "processing",
      title: LONG_CONTENT.korean,
      model_id: LONG_CONTENT.model,
    });
    const markup = renderToStaticMarkup(
      <MeetingTranscriptionView
        {...props({
          active: [active],
          past: [meeting({ id: "meeting-past", title: LONG_CONTENT.token })],
          progressById: { [active.id]: liveProgress },
          liveTextById: { [active.id]: LONG_CONTENT.korean },
        })}
      />,
    );

    expect(markup).toContain(LONG_CONTENT.korean);
    expect(markup).toContain(LONG_CONTENT.token);
    expect(markup).toContain("overflow-wrap-anywhere");
    expect(markup).not.toContain("min-w-0 truncate");
  });

  it("keeps completed and interrupted detail actions distinct", () => {
    const completed = renderToStaticMarkup(
      <MeetingTranscriptionView {...props({ selected: detail() })} />,
    );
    const interrupted = renderToStaticMarkup(
      <MeetingTranscriptionView
        {...props({
          selected: detail({ status: "failed", error: "Model stopped" }),
        })}
      />,
    );

    expect(completed).toContain("Open Folder");
    expect(completed).not.toContain(">Resume<");
    expect(interrupted).toContain("Resume");
    expect(interrupted).toContain("New transcription");
    expect(interrupted).toContain("Model stopped");
  });

  it("configures a recorded meeting with model-aware streaming and speaker separation", async () => {
    const onStartTranscription = vi.fn();
    const models = mockModels().map((model) =>
      model.id === "whisper-medium"
        ? { ...model, status: "ready" as const }
        : model,
    );
    render(
      <MeetingTranscriptionView
        {...props({
          selected: detail({ status: "recorded" }),
          models,
          onStartTranscription,
        })}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Configure transcription" }),
    );
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(
      screen.getByRole("combobox", { name: "Meeting transcription model" }),
    ).toBeTruthy();
    expect(
      screen
        .getByRole("switch", { name: "Streaming" })
        .hasAttribute("disabled"),
    ).toBe(true);
    expect(
      screen
        .getByRole("switch", { name: "Speaker separation" })
        .hasAttribute("disabled"),
    ).toBe(true);

    await userEvent.click(
      screen.getByRole("combobox", { name: "Meeting transcription model" }),
    );
    expect(screen.getByText("Local models")).toBeTruthy();
    expect(screen.getByText("API models")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("option", {
        name: "OpenAI · GPT-4o Transcribe Diarize",
      }),
    );
    await userEvent.click(
      screen.getByRole("switch", { name: "Speaker separation" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start transcription" }),
    );

    expect(onStartTranscription).toHaveBeenCalledWith(
      "meeting-1",
      expect.objectContaining({
        model_id: "openai-gpt-4o-transcribe-diarize",
        speaker_separation_enabled: true,
      }),
    );
  });

  it("configures one request for every selected meeting that can start", async () => {
    const onStartBulkTranscription = vi.fn().mockResolvedValue(true);
    const models = mockModels().map((model) =>
      model.id === "whisper-medium"
        ? { ...model, status: "ready" as const }
        : model,
    );
    render(
      <MeetingTranscriptionView
        {...props({
          past: [
            meeting({
              id: "recorded",
              title: "Recorded sync",
              status: "recorded",
            }),
            meeting({ id: "failed", title: "Failed review", status: "failed" }),
            meeting({ id: "completed", title: "Completed retro" }),
          ],
          models,
          selectedMeetingIds: ["recorded", "failed", "completed"],
          onStartBulkTranscription,
        })}
      />,
    );

    expect(screen.getByText("2 ready · 1 skipped")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Transcribe selected" }),
    );
    expect(screen.getByText("Transcribe 2 meetings")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Start 2 transcriptions" }),
    );

    expect(onStartBulkTranscription).toHaveBeenCalledWith({
      model_id: "whisper-medium",
      language: "auto",
      streaming_enabled: false,
      speaker_separation_enabled: false,
    });
  });

  it("shows persisted and newly committed text in the active detail workspace", () => {
    const selected = detail({
      id: "meeting-active",
      status: "processing",
      progress: {
        segments_total: 12,
        segments_done: 2,
        processed_ms: 1_200_000,
        elapsed_ms: 300_000,
        phase: "transcribing",
      },
    });
    render(
      <MeetingTranscriptionView
        {...props({
          selected,
          progressById: { [selected.record.id]: liveProgress },
          liveTextById: {
            [selected.record.id]: "방금 확정된 세 번째 문장입니다.",
          },
        })}
      />,
    );

    expect(screen.getByText("17% complete")).toBeTruthy();
    expect(screen.getByText("Transcribing")).toBeTruthy();
    expect(screen.getByText(/첫 번째 문장입니다/)).toBeTruthy();
    const liveRegion = screen.getByRole("log", { name: "Live transcript" });
    expect(liveRegion.textContent).toContain("방금 확정된 세 번째 문장입니다.");
    expect(liveRegion.getAttribute("aria-live")).toBe("polite");
  });

  it("defaults to Visible and applies one bulk hide action", async () => {
    const onApplyMeetingSelection = vi.fn();
    render(
      <MeetingTranscriptionView
        {...props({
          past: [meeting({ id: "meeting-1" }), meeting({ id: "meeting-2" })],
          visibleCount: 2,
          hiddenCount: 1,
          selectedMeetingIds: ["meeting-1", "meeting-2"],
          onApplyMeetingSelection,
        })}
      />,
    );

    expect(
      screen
        .getByRole("radio", { name: /Visible2/ })
        .getAttribute("aria-checked"),
    ).toBe("true");
    await userEvent.click(
      screen.getByRole("button", { name: "Hide selected" }),
    );
    expect(onApplyMeetingSelection).toHaveBeenCalledOnce();
  });

  it("restores hidden meetings through one batch action", async () => {
    const onApplyMeetingSelection = vi.fn();
    render(
      <MeetingTranscriptionView
        {...props({
          past: [meeting()],
          visibilityMode: "hidden",
          visibleCount: 2,
          hiddenCount: 1,
          selectedMeetingIds: ["meeting-1"],
          onApplyMeetingSelection,
        })}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Restore selected" }),
    );
    expect(onApplyMeetingSelection).toHaveBeenCalledOnce();
  });

  it("hides an active meeting without cancelling its work", async () => {
    const onCancel = vi.fn();
    const onSetMeetingHidden = vi.fn();
    render(
      <MeetingTranscriptionView
        {...props({
          active: [
            meeting({
              id: "meeting-active",
              status: "processing",
            }),
          ],
          visibleCount: 1,
          onCancel,
          onSetMeetingHidden,
        })}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: "Hide Weekly product review",
      }),
    );
    expect(onSetMeetingHidden).toHaveBeenCalledWith("meeting-active", true);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("replaces destructive deletion with reversible detail visibility", async () => {
    const onSetMeetingHidden = vi.fn();
    render(
      <MeetingTranscriptionView
        {...props({
          selected: detail(),
          visibleCount: 1,
          onSetMeetingHidden,
          visibilityStatus: "Hidden from list · Files remain on disk",
        })}
      />,
    );

    expect(screen.queryByRole("button", { name: /Delete/ })).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: "Hide from list" }),
    );
    expect(onSetMeetingHidden).toHaveBeenCalledWith("meeting-1", true);
    expect(screen.getByRole("status").textContent).toContain(
      "Files remain on disk",
    );
  });
});
