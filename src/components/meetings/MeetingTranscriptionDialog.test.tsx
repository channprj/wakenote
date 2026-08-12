// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockModels } from "@/lib/app-state";
import { MeetingTranscriptionDialog } from "./MeetingTranscriptionDialog";

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

function readyModels() {
  return mockModels().map((model) =>
    model.id === "whisper-medium"
      ? { ...model, status: "ready" as const }
      : model,
  );
}

describe("MeetingTranscriptionDialog", () => {
  it("preserves edited options while the open dialog receives refreshed models", async () => {
    const onSubmit = vi.fn().mockResolvedValue(true);
    const props = {
      open: true,
      onOpenChange: vi.fn(),
      initialModelId: "whisper-medium",
      initialLanguage: "auto" as const,
      targetCount: 1,
      busy: false,
      onSubmit,
    };
    const models = readyModels();
    const { rerender } = render(
      <MeetingTranscriptionDialog {...props} models={models} />,
    );

    await userEvent.click(
      screen.getByRole("combobox", { name: "Meeting transcription model" }),
    );
    await userEvent.click(
      screen.getByRole("option", {
        name: "OpenAI · GPT-4o Transcribe Diarize",
      }),
    );
    await userEvent.click(
      screen.getByRole("combobox", { name: "Meeting transcription language" }),
    );
    await userEvent.click(screen.getByRole("option", { name: "Korean" }));
    await userEvent.click(
      screen.getByRole("switch", { name: "Speaker separation" }),
    );
    await userEvent.click(
      screen.getByRole("switch", { name: "Streaming" }),
    );

    rerender(<MeetingTranscriptionDialog {...props} models={[...models]} />);
    await userEvent.click(
      screen.getByRole("button", { name: "Start transcription" }),
    );

    expect(onSubmit).toHaveBeenCalledWith({
      model_id: "openai-gpt-4o-transcribe-diarize",
      language: "ko",
      streaming_enabled: true,
      speaker_separation_enabled: true,
    });
  });

  it("resets to fresh initial options only after the dialog closes", async () => {
    const onSubmit = vi.fn().mockResolvedValue(true);
    const models = readyModels();
    const baseProps = {
      onOpenChange: vi.fn(),
      models,
      initialModelId: "whisper-medium",
      targetCount: 1,
      busy: false,
      onSubmit,
    };
    const { rerender } = render(
      <MeetingTranscriptionDialog
        {...baseProps}
        open
        initialLanguage="auto"
      />,
    );

    await userEvent.click(
      screen.getByRole("combobox", { name: "Meeting transcription language" }),
    );
    await userEvent.click(screen.getByRole("option", { name: "Korean" }));
    rerender(
      <MeetingTranscriptionDialog
        {...baseProps}
        open={false}
        initialLanguage="en"
      />,
    );
    rerender(
      <MeetingTranscriptionDialog
        {...baseProps}
        open
        initialLanguage="en"
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start transcription" }),
    );

    expect(onSubmit).toHaveBeenCalledWith({
      model_id: "whisper-medium",
      language: "en",
      streaming_enabled: false,
      speaker_separation_enabled: false,
    });
  });

  it("falls back safely when the selected model stops being available", async () => {
    const onSubmit = vi.fn().mockResolvedValue(true);
    const models = readyModels();
    const props = {
      open: true,
      onOpenChange: vi.fn(),
      initialModelId: "whisper-medium",
      initialLanguage: "ko" as const,
      targetCount: 1,
      busy: false,
      onSubmit,
    };
    const { rerender } = render(
      <MeetingTranscriptionDialog {...props} models={models} />,
    );

    await userEvent.click(
      screen.getByRole("combobox", { name: "Meeting transcription model" }),
    );
    await userEvent.click(
      screen.getByRole("option", {
        name: "OpenAI · GPT-4o Transcribe Diarize",
      }),
    );
    await userEvent.click(
      screen.getByRole("switch", { name: "Speaker separation" }),
    );
    await userEvent.click(
      screen.getByRole("switch", { name: "Streaming" }),
    );

    rerender(
      <MeetingTranscriptionDialog
        {...props}
        models={models.filter(
          (model) => model.id !== "openai-gpt-4o-transcribe-diarize",
        )}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start transcription" }),
    );

    expect(onSubmit).toHaveBeenCalledWith({
      model_id: "whisper-medium",
      language: "ko",
      streaming_enabled: false,
      speaker_separation_enabled: false,
    });
  });

  it("submits one capability-normalized request for every target", async () => {
    const onSubmit = vi.fn().mockResolvedValue(true);
    const onOpenChange = vi.fn();
    render(
      <MeetingTranscriptionDialog
        open
        onOpenChange={onOpenChange}
        models={readyModels()}
        initialModelId="whisper-medium"
        initialLanguage="ko"
        targetCount={3}
        busy={false}
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByText("Transcribe 3 meetings")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("combobox", { name: "Meeting transcription model" }),
    );
    await userEvent.click(
      screen.getByRole("option", {
        name: "OpenAI · GPT-4o Transcribe Diarize",
      }),
    );
    await userEvent.click(
      screen.getByRole("switch", { name: "Speaker separation" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Start 3 transcriptions" }),
    );

    expect(onSubmit).toHaveBeenCalledWith({
      model_id: "openai-gpt-4o-transcribe-diarize",
      language: "ko",
      streaming_enabled: false,
      speaker_separation_enabled: true,
    });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("locks required streaming on and submits it as enabled", async () => {
    const onSubmit = vi.fn().mockResolvedValue(true);
    render(
      <MeetingTranscriptionDialog
        open
        onOpenChange={vi.fn()}
        models={readyModels()}
        initialModelId="soniox-realtime-v5"
        initialLanguage="auto"
        targetCount={1}
        busy={false}
        onSubmit={onSubmit}
      />,
    );

    const streaming = screen.getByRole("switch", { name: "Streaming" });
    expect(streaming.getAttribute("aria-checked")).toBe("true");
    expect((streaming as HTMLButtonElement).disabled).toBe(true);
    await userEvent.click(
      screen.getByRole("button", { name: "Start transcription" }),
    );

    expect(onSubmit).toHaveBeenCalledWith({
      model_id: "soniox-realtime-v5",
      language: "auto",
      streaming_enabled: true,
      speaker_separation_enabled: false,
    });
  });

  it("stays open when submission fails", async () => {
    const onOpenChange = vi.fn();
    render(
      <MeetingTranscriptionDialog
        open
        onOpenChange={onOpenChange}
        models={readyModels()}
        initialModelId="whisper-medium"
        initialLanguage="auto"
        targetCount={1}
        busy={false}
        onSubmit={vi.fn().mockResolvedValue(false)}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Start transcription" }),
    );

    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("keeps a submission error visible inside the open dialog", () => {
    render(
      <MeetingTranscriptionDialog
        open
        onOpenChange={vi.fn()}
        models={readyModels()}
        initialModelId="whisper-medium"
        initialLanguage="auto"
        targetCount={2}
        busy={false}
        error="Could not start 1 of 2 selected meetings."
        onSubmit={vi.fn().mockResolvedValue(false)}
      />,
    );

    expect(screen.getByRole("alert").textContent).toContain(
      "Could not start 1 of 2 selected meetings.",
    );
  });
});
