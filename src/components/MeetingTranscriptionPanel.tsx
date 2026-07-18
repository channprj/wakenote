import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  cancelMeeting,
  importAndStartMeeting,
  isTauriRuntime,
  listMeetings,
  meetingDetail,
  openTranscriptFolder,
  resumeMeeting,
} from "../lib/tauri-client";
import { useListVisibility } from "../hooks/use-list-visibility";
import { projectListItems } from "../lib/list-visibility";
import { isMeetingActive } from "../lib/meeting-progress";
import type {
  ListVisibilityTarget,
  MeetingDetail,
  MeetingFinishedPayload,
  MeetingProgressPayload,
  MeetingSegmentPayload,
  MeetingSummary,
} from "../lib/types";
import type { ListVisibilityMode } from "./ListVisibilityToolbar";
import { MeetingTranscriptionView } from "./meetings/MeetingTranscriptionView";

function meetingVisibilityTarget(
  meeting: MeetingSummary,
): ListVisibilityTarget {
  return {
    kind: "meeting",
    id: meeting.id,
  };
}

export function MeetingTranscriptionPanel() {
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [progressById, setProgressById] = useState<Record<string, MeetingProgressPayload>>({});
  const [liveTextById, setLiveTextById] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [visibilityMode, setVisibilityMode] =
    useState<ListVisibilityMode>("visible");
  const [selectedMeetingIds, setSelectedMeetingIds] = useState<
    Set<string>
  >(new Set());
  const visibility = useListVisibility();

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

  const onCopy = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Clipboard can be unavailable; ignore silently.
    }
  }, []);

  const projectedMeetings = useMemo(
    () =>
      projectListItems(
        meetings,
        visibility.state,
        meetingVisibilityTarget,
      ),
    [meetings, visibility.state],
  );
  const displayedMeetings =
    visibilityMode === "visible"
      ? projectedMeetings.visible
      : projectedMeetings.hidden;
  const active = displayedMeetings.filter((meeting) =>
    isMeetingActive(meeting.status),
  );
  const past = displayedMeetings.filter(
    (meeting) => !isMeetingActive(meeting.status),
  );

  useEffect(() => {
    const displayedIds = new Set(
      displayedMeetings.map((meeting) => meeting.id),
    );
    setSelectedMeetingIds((current) => {
      const next = new Set(
        [...current].filter((id) => displayedIds.has(id)),
      );
      return next.size === current.size ? current : next;
    });
  }, [displayedMeetings]);

  const changeVisibilityMode = useCallback(
    (mode: ListVisibilityMode) => {
      setVisibilityMode(mode);
      setSelectedMeetingIds(new Set());
      setSelectedId(null);
      setDetail(null);
    },
    [],
  );

  const changeMeetingSelection = useCallback(
    (id: string, selected: boolean) => {
      setSelectedMeetingIds((current) => {
        const next = new Set(current);
        if (selected) {
          next.add(id);
        } else {
          next.delete(id);
        }
        return next;
      });
    },
    [],
  );

  const applyMeetingVisibility = useCallback(
    async (ids: string[], hidden: boolean) => {
      const succeeded = await visibility.setTargetsHidden(
        ids.map((id) => ({
          kind: "meeting" as const,
          id,
        })),
        hidden,
      );
      if (!succeeded) {
        return;
      }
      setSelectedMeetingIds(new Set());
      if (
        selectedIdRef.current &&
        ids.includes(selectedIdRef.current)
      ) {
        setSelectedId(null);
        setDetail(null);
      }
    },
    [visibility.setTargetsHidden],
  );

  return (
    <MeetingTranscriptionView
      active={active}
      past={past}
      selected={selectedId ? detail : null}
      progressById={progressById}
      liveTextById={liveTextById}
      busy={busy}
      error={error ?? visibility.error}
      visibilityMode={visibilityMode}
      visibleCount={projectedMeetings.visible.length}
      hiddenCount={projectedMeetings.hidden.length}
      selectedMeetingIds={[...selectedMeetingIds]}
      visibilityMutating={
        visibility.loading || visibility.mutating
      }
      visibilityStatus={visibility.announcement}
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
      onVisibilityModeChange={changeVisibilityMode}
      onMeetingSelectionChange={changeMeetingSelection}
      onSelectAllMeetings={() =>
        setSelectedMeetingIds(
          new Set(displayedMeetings.map((meeting) => meeting.id)),
        )
      }
      onClearMeetingSelection={() =>
        setSelectedMeetingIds(new Set())
      }
      onApplyMeetingSelection={() =>
        void applyMeetingVisibility(
          [...selectedMeetingIds],
          visibilityMode === "visible",
        )
      }
      onSetMeetingHidden={(id, hidden) =>
        void applyMeetingVisibility([id], hidden)
      }
    />
  );
}
