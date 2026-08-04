import { useEffect, useMemo, useState } from "react";
import {
  activityAttentionAt,
  type ActivityAttention,
} from "@/lib/activity-attention";
import type { QueueJob } from "@/lib/types";

export interface ActivityAttentionState {
  attention: ActivityAttention | null;
  nowMs: number;
}

export function useActivityAttention(jobs: QueueJob[]): ActivityAttentionState {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const attention = useMemo(
    () => activityAttentionAt(jobs, nowMs),
    [jobs, nowMs],
  );

  useEffect(() => {
    setNowMs(Date.now());
  }, [jobs]);

  useEffect(() => {
    if (attention?.nextExpiryAt == null) {
      return;
    }

    const expiryAt = attention.nextExpiryAt;
    const timer = window.setTimeout(() => {
      setNowMs(Math.max(Date.now(), expiryAt));
    }, Math.max(0, expiryAt - Date.now()));
    return () => window.clearTimeout(timer);
  }, [attention?.nextExpiryAt]);

  return { attention, nowMs };
}
