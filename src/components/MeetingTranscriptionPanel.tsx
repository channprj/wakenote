import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelMeeting,
  deleteMeeting,
  importAndStartMeeting,
  isTauriRuntime,
  listMeetings,
  meetingDetail,
  openTranscriptFolder,
  resumeMeeting,
} from "../lib/tauri-client";
import { isMeetingActive } from "../lib/meeting-progress";
import type {
  MeetingDetail,
  MeetingFinishedPayload,
  MeetingProgressPayload,
  MeetingSegmentPayload,
  MeetingSummary,
} from "../lib/types";
import { MeetingTranscriptionView } from "./meetings/MeetingTranscriptionView";

export function MeetingTranscriptionPanel() {
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [progressById, setProgressById] = useState<Record<string, MeetingProgressPayload>>({});
  const [liveTextById, setLiveTextById] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedIdRef = useRef<string | null>(null);
  selectedIdRef.current = selectedId;

  const refreshMeetings = useCallback(async () => {
    try {
      setMeetings(await listMeetings());
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  const openDetail = useCallback(async (id: string) => {
    try {
      const next = await meetingDetail(id);
      setDetail(next);
      setSelectedId(id);
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  useEffect(() => {
    void refreshMeetings();
    if (!isTauriRuntime()) {
      return;
    }
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const onProgress = await listen<MeetingProgressPayload>("meeting-progress", (event) => {
        const payload = event.payload;
        setProgressById((prev) => ({ ...prev, [payload.id]: payload }));
      });
      const onSegment = await listen<MeetingSegmentPayload>(
        "meeting-segment-committed",
        (event) => {
          const payload = event.payload;
          setLiveTextById((prev) => {
            const existing = prev[payload.id];
            const text = existing ? `${existing}\n${payload.text}` : payload.text;
            return { ...prev, [payload.id]: text.trim() };
          });
        },
      );
      const onFinished = await listen<MeetingFinishedPayload>("meeting-finished", (event) => {
        const payload = event.payload;
        setProgressById((prev) => {
          const next = { ...prev };
          delete next[payload.id];
          return next;
        });
        void refreshMeetings();
        if (selectedIdRef.current === payload.id) {
          void openDetail(payload.id);
        }
      });
      if (disposed) {
        onProgress();
        onSegment();
        onFinished();
        return;
      }
      unlisteners.push(onProgress, onSegment, onFinished);
    })();
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [refreshMeetings, openDetail]);

  const onImport = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const summary = await importAndStartMeeting();
      if (summary) {
        setLiveTextById((prev) => ({ ...prev, [summary.id]: "" }));
        await refreshMeetings();
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [refreshMeetings]);

  const onCancel = useCallback(async (id: string) => {
    try {
      await cancelMeeting(id);
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  const onResume = useCallback(
    async (id: string) => {
      setError(null);
      try {
        await resumeMeeting(id);
        await refreshMeetings();
      } catch (cause) {
        setError(String(cause));
      }
    },
    [refreshMeetings],
  );

  const onDelete = useCallback(
    async (id: string) => {
      try {
        await deleteMeeting(id);
        if (selectedIdRef.current === id) {
          setSelectedId(null);
          setDetail(null);
        }
        await refreshMeetings();
      } catch (cause) {
        setError(String(cause));
      }
    },
    [refreshMeetings],
  );

  const onCopy = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Clipboard can be unavailable; ignore silently.
    }
  }, []);

  const active = meetings.filter((meeting) => isMeetingActive(meeting.status));
  const past = meetings.filter((meeting) => !isMeetingActive(meeting.status));

  return (
    <MeetingTranscriptionView
      active={active}
      past={past}
      selected={selectedId ? detail : null}
      progressById={progressById}
      liveTextById={liveTextById}
      busy={busy}
      error={error}
      onImport={() => void onImport()}
      onOpen={(id) => void openDetail(id)}
      onBack={() => {
        setSelectedId(null);
        setDetail(null);
      }}
      onCancel={(id) => void onCancel(id)}
      onResume={(id) => void onResume(id)}
      onCopy={(text) => void onCopy(text)}
      onOpenFolder={(audioPath) => void openTranscriptFolder(audioPath)}
      onDelete={(id) => void onDelete(id)}
    />
  );
}
