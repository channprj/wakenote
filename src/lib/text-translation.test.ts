import { describe, expect, it, vi } from "vitest";
import { requestTranslation } from "./text-translation";
import { transformText } from "./tauri-client";

vi.mock("./tauri-client", () => ({
  transformText: vi.fn(),
  cancelTextTransform: vi.fn().mockResolvedValue(undefined),
}));

describe("translation request scheduling", () => {
  it("limits concurrent requests and never sends a cancelled waiting item", async () => {
    const finish: Array<() => void> = [];
    vi.mocked(transformText).mockImplementation(
      (_id, request) =>
        new Promise((resolve) => {
          finish.push(() =>
            resolve({
              kind: "translate",
              text: request.text,
              model: "model",
              target_language: "ko",
            }),
          );
        }),
    );
    const first = requestTranslation("queue first", "ko", "model");
    const second = requestTranslation("queue second", "ko", "model");
    const third = requestTranslation("queue third", "ko", "model");
    const cancelled = third.promise.catch((error: Error) => error.message);
    expect(transformText).toHaveBeenCalledTimes(2);
    third.cancel();
    finish.forEach((resolve) => resolve());
    await Promise.all([first.promise, second.promise]);
    expect(await cancelled).toBe("Translation cancelled");
    expect(transformText).toHaveBeenCalledTimes(2);
    await requestTranslation("queue first", "ko", "model").promise;
    expect(transformText).toHaveBeenCalledTimes(2);
  });
});
