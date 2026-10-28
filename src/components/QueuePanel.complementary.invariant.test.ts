import { describe, expect, it } from "vitest";
import {
  cancelCurrentDisabledReason,
  processNextDisabledReason,
  queueJobActionState,
  queueJobRetryDisabledReason,
  queueJobSkipDisabledReason,
  queueToolbarActionState,
} from "./QueuePanel";
import type { QueueJobStatus } from "../lib/types";

// Pin the implicit contract that complementary actionState + disabledReason helpers must agree
// on whether an action is enabled: canX === true ⟺ <action>DisabledReason(...) === null.
// The implementation enforces this by construction today (e.g. queueJobRetryDisabledReason
// returns null when queueJobActionState(status).canRetry is true), but the per-helper tests
// pin each side's truth table independently — a future refactor that decouples them could
// silently desync the UI (button enabled while tooltip says "cannot do X", or vice versa).

const QUEUE_JOB_STATUSES = [
  "pending",
  "running",
  "completed",
  "failed",
  "cancelled",
  "skipped",
] as const satisfies readonly QueueJobStatus[];

describe("queueJobActionState ↔ per-row disabled-reason invariants", () => {
  it("keeps every row action flag in agreement with its disabled reason", () => {
    for (const status of QUEUE_JOB_STATUSES) {
      const { canRetry, canSkip } = queueJobActionState(status);
      expect(
        canRetry,
        `canRetry vs queueJobRetryDisabledReason for ${status}`,
      ).toBe(queueJobRetryDisabledReason(status) === null);
      expect(
        canSkip,
        `canSkip vs queueJobSkipDisabledReason for ${status}`,
      ).toBe(queueJobSkipDisabledReason(status) === null);
    }
  });
});

const TOOLBAR_CASES: Array<[number, number, boolean]> = [];
for (const pending_count of [0, 1, 3]) {
  for (const running_count of [0, 1]) {
    for (const canProcess of [true, false]) {
      TOOLBAR_CASES.push([pending_count, running_count, canProcess]);
    }
  }
}

describe("queueToolbarActionState ↔ toolbar disabled-reason invariants", () => {
  it("keeps every toolbar action flag in agreement with its disabled reason", () => {
    for (const [pending_count, running_count, canProcess] of TOOLBAR_CASES) {
      const label = `pending=${pending_count} running=${running_count} canProcess=${canProcess}`;
      const queue = { jobs: [], pending_count, running_count, failed_count: 0 };
      const { canProcessNext, canCancelCurrent } = queueToolbarActionState(
        queue,
        canProcess,
      );
      expect(
        canProcessNext,
        `canProcessNext vs processNextDisabledReason for ${label}`,
      ).toBe(processNextDisabledReason(queue, canProcess) === null);
      expect(
        canCancelCurrent,
        `canCancelCurrent vs cancelCurrentDisabledReason for ${label}`,
      ).toBe(cancelCurrentDisabledReason(queue) === null);
    }
  });
});
