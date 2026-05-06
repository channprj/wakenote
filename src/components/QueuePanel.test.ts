import { describe, expect, it } from "vitest";
import { queueJobActionState } from "./QueuePanel";
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
