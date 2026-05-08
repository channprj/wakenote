import { describe, expect, it } from "vitest";
import { queueJobActionState, queueToolbarActionState } from "./QueuePanel";
import type { QueueJobStatus } from "../lib/types";

describe("queue row actions", () => {
  it.each([
    ["pending", false, true],
    ["running", false, false],
    ["completed", false, false],
    ["failed", true, true],
    ["cancelled", true, true],
    ["skipped", false, false],
  ] satisfies Array<[QueueJobStatus, boolean, boolean]>)(
    "derives valid actions for %s jobs",
    (status, canRetry, canSkip) => {
      expect(queueJobActionState(status)).toEqual({ canRetry, canSkip });
    },
  );
});

describe("queue toolbar actions", () => {
  it("enables process next only when pending work can be transcribed", () => {
    const queue = {
      jobs: [],
      pending_count: 1,
      running_count: 0,
      failed_count: 0,
    };

    expect(queueToolbarActionState(queue, true)).toEqual({ canProcessNext: true });
    expect(queueToolbarActionState(queue, false)).toEqual({ canProcessNext: false });
  });

  it("keeps process next disabled when no pending work exists", () => {
    const queue = {
      jobs: [],
      pending_count: 0,
      running_count: 1,
      failed_count: 0,
    };

    expect(queueToolbarActionState(queue, true)).toEqual({ canProcessNext: false });
  });
});
