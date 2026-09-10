// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TranslatedText } from "./TranslatedText";
import { transformText, cancelTextTransform } from "@/lib/tauri-client";

vi.mock("@/lib/tauri-client", () => ({
  transformText: vi.fn(),
  cancelTextTransform: vi.fn().mockResolvedValue(undefined),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("transcript translation", () => {
  it("shows only the original text when automatic translation is off", () => {
    render(
      <TranslatedText
        text="Original one"
        preferences={{
          enabled: false,
          language: "ko",
          model: "model",
          configured: true,
        }}
      />,
    );
    expect(transformText).not.toHaveBeenCalled();
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("Original one")).toBeTruthy();
  });

  it("ignores a late result after the target language changes and cancels old work", async () => {
    let resolveOld!: (value: Awaited<ReturnType<typeof transformText>>) => void;
    vi.mocked(transformText)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveOld = resolve;
          }),
      )
      .mockResolvedValueOnce({
        kind: "translate",
        text: "新しい翻訳",
        target_language: "ja",
        model: "model",
      });
    const view = render(
      <TranslatedText
        text="Original two"
        preferences={{
          enabled: true,
          language: "ko",
          model: "model",
          configured: true,
        }}
      />,
    );
    await waitFor(() => expect(transformText).toHaveBeenCalledTimes(1));
    view.rerender(
      <TranslatedText
        text="Original two"
        preferences={{
          enabled: true,
          language: "ja",
          model: "model",
          configured: true,
        }}
      />,
    );
    expect(await screen.findByText("新しい翻訳")).toBeTruthy();
    await act(async () =>
      resolveOld({
        kind: "translate",
        text: "late Korean",
        target_language: "ko",
        model: "model",
      }),
    );
    expect(screen.queryByText("late Korean")).toBeNull();
    expect(cancelTextTransform).toHaveBeenCalled();
  });

  it("keeps source text and offers retry after a failed request", async () => {
    vi.mocked(transformText)
      .mockRejectedValueOnce(new Error("Provider unavailable"))
      .mockResolvedValueOnce({
        kind: "translate",
        text: "재시도 성공",
        target_language: "ko",
        model: "model",
      });
    render(
      <TranslatedText
        text="Original three"
        preferences={{
          enabled: true,
          language: "ko",
          model: "model",
          configured: true,
        }}
      />,
    );
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "Provider unavailable",
    );
    expect(screen.getByText("Original three")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry translation" }));
    expect(await screen.findByText("재시도 성공")).toBeTruthy();
  });
});

describe("unset transcript translation language", () => {
  const preferences = {
    enabled: true,
    language: "ko" as const,
    configured: true,
    model: "model",
  };

  it.each([null, "auto"] as const)(
    "preserves source text without requesting translation for %s",
    (language) => {
      render(
        <TranslatedText
          text="Unset target source"
          preferences={{ ...preferences, language }}
        />,
      );
      expect(screen.getByText("Unset target source")).toBeTruthy();
      expect(transformText).not.toHaveBeenCalled();
      expect(screen.queryByRole("status")).toBeNull();
    },
  );

  it("cancels pending work and ignores its result when the target is cleared", async () => {
    let finish!: (value: Awaited<ReturnType<typeof transformText>>) => void;
    vi.mocked(transformText).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const { rerender } = render(
      <TranslatedText text="Pending target source" preferences={preferences} />,
    );
    await waitFor(() => expect(transformText).toHaveBeenCalledOnce());
    expect(screen.getByText("Translating…")).toBeTruthy();
    rerender(
      <TranslatedText
        text="Pending target source"
        preferences={{ ...preferences, language: null }}
      />,
    );
    expect(cancelTextTransform).toHaveBeenCalledOnce();
    await act(async () =>
      finish({
        kind: "translate",
        text: "Late translation",
        target_language: "ko",
        model: "model",
      }),
    );
    expect(screen.queryByText("Late translation")).toBeNull();
    expect(screen.getByText("Pending target source")).toBeTruthy();
    expect(screen.queryByRole("status")).toBeNull();
    expect(transformText).toHaveBeenCalledOnce();
  });

  it("hides an existing translation when the target is cleared", async () => {
    vi.mocked(transformText).mockResolvedValueOnce({
      kind: "translate",
      text: "Completed translation",
      target_language: "ko",
      model: "model",
    });
    const { rerender } = render(
      <TranslatedText
        text="Completed target source"
        preferences={preferences}
      />,
    );
    expect(await screen.findByText("Completed translation")).toBeTruthy();
    rerender(
      <TranslatedText
        text="Completed target source"
        preferences={{ ...preferences, language: null }}
      />,
    );
    expect(screen.queryByText("Completed translation")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Copy translation" }),
    ).toBeNull();
    expect(screen.getByText("Completed target source")).toBeTruthy();
  });
});
