// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { MeetingDetail, MeetingProgressPayload, MeetingSummary } from "@/lib/types";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import {
  MeetingTranscriptionView,
  type MeetingTranscriptionViewProps,
} from "./MeetingTranscriptionView";

afterEach(cleanup);

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
    },
    transcript: "첫 번째 문장입니다.\n두 번째 문장입니다.",
    audio_path: "/tmp/weekly-review.m4a",
  };
}

const liveProgress: MeetingProgressPayload = {
  id: "meeting-active",
  status: "processing",
  segments_total: 12,
  segments_done: 2,
  processed_ms: 1_200_000,
  duration_ms: 7_200_000,
  elapsed_ms: 300_000,
  eta_ms: 900_000,
};

function props(overrides: Partial<MeetingTranscriptionViewProps> = {}): MeetingTranscriptionViewProps {
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
    error: null,
    visibilityMode: "visible",
    visibleCount: 0,
    hiddenCount: 0,
    selectedMeetingIds: [],
    visibilityMutating: false,
    visibilityStatus: "",
    onImport: vi.fn(),
    onStartManualRecording: vi.fn(),
    onStopManualRecording: vi.fn(),
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
    expect(markup).toContain("Remaining");
    expect(markup).toContain("현재 회의 내용을 전사");
  });

  it("truncates long past titles while exposing the complete title", () => {
    const longTitle = "아주 긴 회의 제목이 목록 너비를 넘어가더라도 전체 제목을 확인할 수 있어야 합니다";
    const markup = renderToStaticMarkup(
      <MeetingTranscriptionView {...props({ past: [meeting({ title: longTitle })] })} />,
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
        {...props({ selected: detail({ status: "failed", error: "Model stopped" }) })}
      />,
    );

    expect(completed).toContain("Open Folder");
    expect(completed).not.toContain(">Resume<");
    expect(interrupted).toContain("Resume");
    expect(interrupted).toContain("Model stopped");
  });

  it("defaults to Visible and applies one bulk hide action", async () => {
    const onApplyMeetingSelection = vi.fn();
    render(
      <MeetingTranscriptionView
        {...props({
          past: [
            meeting({ id: "meeting-1" }),
            meeting({ id: "meeting-2" }),
          ],
          visibleCount: 2,
          hiddenCount: 1,
          selectedMeetingIds: ["meeting-1", "meeting-2"],
          onApplyMeetingSelection,
        })}
      />,
    );

    expect(
      screen.getByRole("tab", { name: /Visible2/ }).getAttribute(
        "aria-selected",
      ),
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
    expect(onSetMeetingHidden).toHaveBeenCalledWith(
      "meeting-active",
      true,
    );
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
          visibilityStatus:
            "Hidden from list · Files remain on disk",
        })}
      />,
    );

    expect(
      screen.queryByRole("button", { name: /Delete/ }),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: "Hide from list" }),
    );
    expect(onSetMeetingHidden).toHaveBeenCalledWith(
      "meeting-1",
      true,
    );
    expect(screen.getByRole("status").textContent).toContain(
      "Files remain on disk",
    );
  });
});
