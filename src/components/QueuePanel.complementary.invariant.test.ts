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
  it.each(QUEUE_JOB_STATUSES)(
    "canRetry agrees with queueJobRetryDisabledReason for %s",
    (status) => {
      const { canRetry } = queueJobActionState(status);
      const reason = queueJobRetryDisabledReason(status);
      expect(canRetry).toBe(reason === null);
    },
  );

  it.each(QUEUE_JOB_STATUSES)(
    "canSkip agrees with queueJobSkipDisabledReason for %s",
    (status) => {
      const { canSkip } = queueJobActionState(status);
      const reason = queueJobSkipDisabledReason(status);
      expect(canSkip).toBe(reason === null);
    },
  );
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
  it.each(TOOLBAR_CASES)(
    "canProcessNext agrees with processNextDisabledReason for pending=%i running=%i canProcess=%s",
    (pending_count, running_count, canProcess) => {
      const queue = { jobs: [], pending_count, running_count, failed_count: 0 };
      const { canProcessNext } = queueToolbarActionState(queue, canProcess);
      const reason = processNextDisabledReason(queue, canProcess);
      expect(canProcessNext).toBe(reason === null);
    },
  );

  it.each(TOOLBAR_CASES)(
    "canCancelCurrent agrees with cancelCurrentDisabledReason for pending=%i running=%i canProcess=%s",
    (pending_count, running_count, _canProcess) => {
      const queue = { jobs: [], pending_count, running_count, failed_count: 0 };
      const { canCancelCurrent } = queueToolbarActionState(queue, _canProcess);
      const reason = cancelCurrentDisabledReason(queue);
      expect(canCancelCurrent).toBe(reason === null);
    },
  );
});
