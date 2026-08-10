import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  cancelMeeting,
  importMeetingRecording,
  isTauriRuntime,
  listMeetings,
  loadManualMeetingRecordingStatus,
  meetingDetail,
  openTranscriptFolder,
  resumeMeeting,
  startMeetingTranscription,
  startManualMeetingRecording,
  stopManualMeetingRecording,
} from "../lib/tauri-client";
import { useListVisibility } from "../hooks/use-list-visibility";
import { subscribeMeetingEvents } from "../lib/meeting-event-subscriptions";
import { projectListItems } from "../lib/list-visibility";
import { isMeetingActive } from "../lib/meeting-progress";
import type { PermissionFeature } from "../lib/permission-guidance";
import type {
  ListVisibilityTarget,
  ManualMeetingRecordingStatus,
  MeetingDetail,
  MeetingTranscriptionRequest,
  MeetingProgressPayload,
  MeetingSummary,
  ModelDescriptor,
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

export function MeetingTranscriptionPanel({
  onPermissionRequired,
  models,
  meetingModelId,
  selectedModelId,
  onMeetingModelChange,
}: {
  onPermissionRequired: (feature: PermissionFeature) => Promise<boolean>;
  models: ModelDescriptor[];
  meetingModelId: string;
  selectedModelId: string;
  onMeetingModelChange: (modelId: string) => void;
}) {
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [progressById, setProgressById] = useState<Record<string, MeetingProgressPayload>>({});
  const [liveTextById, setLiveTextById] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [configureOnOpenId, setConfigureOnOpenId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [manualRecordingBusy, setManualRecordingBusy] = useState(false);
  const [transcriptionBusy, setTranscriptionBusy] = useState(false);
  const [manualRecording, setManualRecording] =
    useState<ManualMeetingRecordingStatus>({
      generation: 0,
      state: "off",
      meeting_id: null,
      started_at: null,
      elapsed_ms: 0,
      remaining_ms: 18_000_000,
      inputs: ["Microphone", "System Audio"],
      stop_reason: null,
      error: null,
    });
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

  const refreshManualRecording = useCallback(async () => {
    try {
      setManualRecording(await loadManualMeetingRecordingStatus());
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
    if (!isTauriRuntime()) {
      void refreshMeetings();
      return;
    }
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const nextUnlisteners = await subscribeMeetingEvents(listen, {
          onProgress: (payload) => {
            setProgressById((prev) => ({ ...prev, [payload.id]: payload }));
          },
          onSegment: (payload) => {
            setLiveTextById((prev) => {
              const existing = prev[payload.id];
              const text = existing ? `${existing}\n${payload.text}` : payload.text;
              return { ...prev, [payload.id]: text.trim() };
            });
          },
          onFinished: (payload) => {
            setProgressById((prev) => {
              const next = { ...prev };
              delete next[payload.id];
              return next;
            });
            void refreshMeetings();
            if (selectedIdRef.current === payload.id) {
              void openDetail(payload.id);
            }
          },
        });
        if (disposed) {
          for (const unlisten of nextUnlisteners) {
            unlisten();
          }
          return;
        }
        unlisteners.push(...nextUnlisteners);
        await refreshMeetings();
      } catch (cause) {
        if (!disposed) {
          setError(String(cause));
        }
      }
    })();
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [refreshMeetings, openDetail]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void refreshManualRecording();
    if (isTauriRuntime()) {
      void import("@tauri-apps/api/event")
        .then(({ listen }) =>
          listen<ManualMeetingRecordingStatus>(
            "manual-meeting-recording-state",
            (event) => {
              if (!disposed) {
                setManualRecording(event.payload);
                if (event.payload.state !== "recording") {
                  void refreshMeetings();
                }
              }
            },
          ),
        )
        .then((stop) => {
          if (disposed) {
            stop();
          } else {
            unlisten = stop;
          }
        })
        .catch((cause) => {
          if (!disposed) {
            setError(String(cause));
          }
        });
    }
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshManualRecording, refreshMeetings]);

  useEffect(() => {
    if (manualRecording.state !== "recording") {
      return;
    }
    const timer = window.setInterval(() => {
      void refreshManualRecording();
    }, 1_000);
    return () => window.clearInterval(timer);
  }, [manualRecording.state, refreshManualRecording]);

  const onStartManualRecording = useCallback(async () => {
    if (!(await onPermissionRequired("manual_meeting"))) {
      return;
    }
    setManualRecordingBusy(true);
    setError(null);
    try {
      setManualRecording(await startManualMeetingRecording());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setManualRecordingBusy(false);
    }
  }, [onPermissionRequired]);

  const onStopManualRecording = useCallback(async () => {
    setManualRecordingBusy(true);
    setError(null);
    try {
      const status = await stopManualMeetingRecording();
      setManualRecording(status);
      await refreshMeetings();
      if (status.meeting_id) {
        await openDetail(status.meeting_id);
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setManualRecordingBusy(false);
    }
  }, [openDetail, refreshMeetings]);

  const onStartTranscription = useCallback(
    async (id: string, request: MeetingTranscriptionRequest) => {
      setTranscriptionBusy(true);
      setError(null);
      try {
        await startMeetingTranscription(id, request);
        await refreshMeetings();
        await openDetail(id);
      } catch (cause) {
        setError(String(cause));
      } finally {
        setTranscriptionBusy(false);
      }
    },
    [openDetail, refreshMeetings],
  );

  const onImport = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const summary = await importMeetingRecording();
      if (summary) {
        setLiveTextById((prev) => ({ ...prev, [summary.id]: "" }));
        await refreshMeetings();
        setConfigureOnOpenId(summary.id);
        await openDetail(summary.id);
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [openDetail, refreshMeetings]);

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
      manualRecording={manualRecording}
      manualRecordingBusy={manualRecordingBusy}
      models={models}
      meetingModelId={meetingModelId}
      selectedModelId={selectedModelId}
      configurationInitiallyOpen={
        selectedId !== null && selectedId === configureOnOpenId
      }
      transcriptionBusy={transcriptionBusy}
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
      onMeetingModelChange={onMeetingModelChange}
      onStartManualRecording={() => void onStartManualRecording()}
      onStopManualRecording={() => void onStopManualRecording()}
      onStartTranscription={(id, request) =>
        void onStartTranscription(id, request)
      }
      onOpen={(id) => void openDetail(id)}
      onBack={() => {
        setConfigureOnOpenId(null);
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
