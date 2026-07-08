import { describe, expect, it } from "vitest";
import {
  OVERLAY_FINAL_HOLD_MS,
  hideCaptionForGeneration,
  initialCaptionState,
  reduceCaptionOverlay,
} from "./caption-state";

describe("overlay caption state", () => {
  it("waits for real transcript text before showing the overlay", () => {
    const state = reduceCaptionOverlay(initialCaptionState(), {
      type: "started",
      chunk_id: 7,
      started_at: "2026-07-08T12:00:00Z",
    });

    expect(state.visible).toBe(false);
    expect(state.text).toBe("");
    expect(state.chunkId).toBe(7);
  });

  it("shows only the current partial text for the active chunk", () => {
    const started = reduceCaptionOverlay(initialCaptionState(), {
      type: "started",
      chunk_id: 7,
      started_at: "2026-07-08T12:00:00Z",
    });

    const state = reduceCaptionOverlay(started, {
      type: "partial",
      chunk_id: 7,
      text: "  지금 말하고 있는 내용입니다  ",
    });

    expect(state.visible).toBe(true);
    expect(state.status).toBe("partial");
    expect(state.text).toBe("지금 말하고 있는 내용입니다");
  });

  it("keeps the latest partial visible while the accurate final pass is running", () => {
    const partial = reduceCaptionOverlay(
      reduceCaptionOverlay(initialCaptionState(), {
        type: "started",
        chunk_id: 7,
        started_at: "2026-07-08T12:00:00Z",
      }),
      {
        type: "partial",
        chunk_id: 7,
        text: "초안 자막",
      },
    );

    const state = reduceCaptionOverlay(partial, {
      type: "committed",
      chunk_id: 7,
      audio_path: "/tmp/voice.wav",
      will_transcribe: true,
    });

    expect(state.visible).toBe(true);
    expect(state.status).toBe("refining");
    expect(state.text).toBe("초안 자막");
    expect(state.hideAfterMs).toBeNull();
  });

  it("replaces the partial with final text and schedules a short caption hold", () => {
    const refining = reduceCaptionOverlay(
      reduceCaptionOverlay(
        reduceCaptionOverlay(initialCaptionState(), {
          type: "started",
          chunk_id: 7,
          started_at: "2026-07-08T12:00:00Z",
        }),
        {
          type: "partial",
          chunk_id: 7,
          text: "초안 자막",
        },
      ),
      {
        type: "committed",
        chunk_id: 7,
        audio_path: "/tmp/voice.wav",
        will_transcribe: true,
      },
    );

    const state = reduceCaptionOverlay(refining, {
      type: "final",
      chunk_id: 7,
      audio_path: "/tmp/voice.wav",
      text: "최종 자막",
      recorded_at: "2026-07-08T12:00:03Z",
    });

    expect(state.visible).toBe(true);
    expect(state.status).toBe("final");
    expect(state.text).toBe("최종 자막");
    expect(state.hideAfterMs).toBe(OVERLAY_FINAL_HOLD_MS);
  });

  it("does not let a stale hide timer clear a newer utterance", () => {
    const final = reduceCaptionOverlay(
      reduceCaptionOverlay(initialCaptionState(), {
        type: "started",
        chunk_id: 7,
        started_at: "2026-07-08T12:00:00Z",
      }),
      {
        type: "final",
        chunk_id: 7,
        audio_path: "/tmp/voice.wav",
        text: "이전 자막",
      },
    );
    const staleGeneration = final.generation;
    const next = reduceCaptionOverlay(final, {
      type: "started",
      chunk_id: 8,
      started_at: "2026-07-08T12:00:06Z",
    });
    const withNextText = reduceCaptionOverlay(next, {
      type: "partial",
      chunk_id: 8,
      text: "새 자막",
    });

    expect(hideCaptionForGeneration(withNextText, staleGeneration)).toBe(withNextText);
    expect(hideCaptionForGeneration(withNextText, withNextText.generation).visible).toBe(false);
  });
});
