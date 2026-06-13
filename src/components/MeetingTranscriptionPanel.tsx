import {
  AlertCircle,
  ChevronLeft,
  Copy,
  FileAudio,
  FolderOpen,
  Loader2,
  RotateCcw,
  Trash2,
  Upload,
} from "lucide-react";
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
import {
  canResumeMeeting,
  formatClock,
  formatEta,
  isMeetingActive,
  meetingStatusLabel,
  meetingStatusTone,
  progressPercent,
} from "../lib/meeting-progress";
import type {
  MeetingDetail,
  MeetingFinishedPayload,
  MeetingProgressPayload,
  MeetingSegmentPayload,
  MeetingSummary,
} from "../lib/types";
import { Badge, Button, Progress } from "./ui/primitives";

function formatDate(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}

export function MeetingTranscriptionPanel() {
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [progressById, setProgressById] = useState<Record<string, MeetingProgressPayload>>({});
  const [liveTextById, setLiveTextById] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

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
        setConfirmDeleteId(null);
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

  if (selectedId && detail) {
    return (
      <MeetingDetailView
        detail={detail}
        confirmingDelete={confirmDeleteId === selectedId}
        onBack={() => {
          setSelectedId(null);
          setDetail(null);
          setConfirmDeleteId(null);
        }}
        onResume={() => onResume(selectedId)}
        onCopy={() => onCopy(detail.transcript)}
        onOpenFolder={() => void openTranscriptFolder(detail.audio_path)}
        onRequestDelete={() => setConfirmDeleteId(selectedId)}
        onCancelDelete={() => setConfirmDeleteId(null)}
        onConfirmDelete={() => onDelete(selectedId)}
      />
    );
  }

  const active = meetings.filter((meeting) => isMeetingActive(meeting.status));
  const past = meetings.filter((meeting) => !isMeetingActive(meeting.status));

  return (
    <div className="meeting-panel">
      <div className="meeting-panel__toolbar">
        <Button onClick={onImport} disabled={busy}>
          {busy ? <Loader2 data-icon="inline-start" className="meeting-spin" /> : <Upload data-icon="inline-start" />}
          회의 파일 선택 (1–2시간)
        </Button>
        <span className="meeting-panel__hint">긴 녹음을 구간별로 전사합니다 · 화자 분리 없음</span>
      </div>

      {error ? (
        <p className="meeting-error" role="alert">
          <AlertCircle data-icon="inline-start" />
          {error}
        </p>
      ) : null}

      {active.map((meeting) => (
        <MeetingProgressCard
          key={meeting.id}
          meeting={meeting}
          live={progressById[meeting.id]}
          previewText={liveTextById[meeting.id] ?? ""}
          onCancel={() => onCancel(meeting.id)}
        />
      ))}

      <div className="meeting-list">
        <p className="meeting-list__title">지난 회의</p>
        {past.length === 0 ? (
          <div className="meeting-empty">
            <FileAudio aria-hidden />
            <span>아직 전사한 회의가 없습니다.</span>
          </div>
        ) : (
          <ul>
            {past.map((meeting) => (
              <li key={meeting.id}>
                <button type="button" className="meeting-row" onClick={() => openDetail(meeting.id)}>
                  <span className="meeting-row__main">
                    <span className="meeting-row__title">{meeting.title}</span>
                    <span className="meeting-row__meta">
                      {formatClock(meeting.duration_ms)} · {formatDate(meeting.created_at)}
                    </span>
                  </span>
                  <Badge tone={meetingStatusTone(meeting.status)}>
                    {meetingStatusLabel(meeting.status)}
                  </Badge>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function MeetingProgressCard({
  meeting,
  live,
  previewText,
  onCancel,
}: {
  meeting: MeetingSummary;
  live: MeetingProgressPayload | undefined;
  previewText: string;
  onCancel: () => void;
}) {
  const processed = live?.processed_ms ?? meeting.progress.processed_ms;
  const duration = live?.duration_ms ?? meeting.duration_ms;
  const segDone = live?.segments_done ?? meeting.progress.segments_done;
  const segTotal = live?.segments_total ?? meeting.progress.segments_total;
  const elapsed = live?.elapsed_ms ?? meeting.progress.elapsed_ms;
  const eta = live?.eta_ms ?? 0;
  const percent = progressPercent(processed, duration);

  return (
    <div className="meeting-card">
      <div className="meeting-card__head">
        <span className="meeting-card__title">
          <Loader2 data-icon="inline-start" className="meeting-spin" />
          {meeting.title}
        </span>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          취소
        </Button>
      </div>
      <Progress value={percent} />
      <div className="meeting-card__meta">
        <span>{percent}%</span>
        <span>
          구간 {segDone}/{segTotal || "?"}
        </span>
        <span>경과 {formatClock(elapsed)}</span>
        <span>남은 시간 {formatEta(eta)}</span>
      </div>
      {previewText ? (
        <div className="meeting-card__preview" aria-label="실시간 전사 미리보기">
          {previewText}
        </div>
      ) : (
        <div className="meeting-card__preview meeting-card__preview--empty">
          첫 구간을 전사하는 중입니다…
        </div>
      )}
    </div>
  );
}

function MeetingDetailView({
  detail,
  confirmingDelete,
  onBack,
  onResume,
  onCopy,
  onOpenFolder,
  onRequestDelete,
  onCancelDelete,
  onConfirmDelete,
}: {
  detail: MeetingDetail;
  confirmingDelete: boolean;
  onBack: () => void;
  onResume: () => void;
  onCopy: () => void;
  onOpenFolder: () => void;
  onRequestDelete: () => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
}) {
  const { record, transcript } = detail;
  return (
    <div className="meeting-detail">
      <div className="meeting-detail__head">
        <Button variant="ghost" size="sm" onClick={onBack}>
          <ChevronLeft data-icon="inline-start" />
          목록
        </Button>
        <span className="meeting-detail__title">{record.title}</span>
        <Badge tone={meetingStatusTone(record.status)}>{meetingStatusLabel(record.status)}</Badge>
      </div>

      <div className="meeting-detail__meta">
        <span>{formatClock(record.duration_ms)}</span>
        <span>{record.model_id}</span>
        <span>{formatDate(record.created_at)}</span>
        <span>
          구간 {record.progress.segments_done}/{record.progress.segments_total}
        </span>
      </div>

      {record.error ? (
        <p className="meeting-error" role="alert">
          <AlertCircle data-icon="inline-start" />
          {record.error}
        </p>
      ) : null}

      <div className="meeting-detail__actions">
        {canResumeMeeting(record.status) ? (
          <Button variant="secondary" size="sm" onClick={onResume}>
            <RotateCcw data-icon="inline-start" />
            이어서 전사
          </Button>
        ) : null}
        <Button variant="secondary" size="sm" onClick={onCopy} disabled={!transcript}>
          <Copy data-icon="inline-start" />
          복사
        </Button>
        <Button variant="secondary" size="sm" onClick={onOpenFolder}>
          <FolderOpen data-icon="inline-start" />
          폴더 열기
        </Button>
        {confirmingDelete ? (
          <>
            <Button variant="danger" size="sm" onClick={onConfirmDelete}>
              삭제 확인
            </Button>
            <Button variant="ghost" size="sm" onClick={onCancelDelete}>
              취소
            </Button>
          </>
        ) : (
          <Button variant="ghost" size="sm" onClick={onRequestDelete}>
            <Trash2 data-icon="inline-start" />
            삭제
          </Button>
        )}
      </div>

      <div className="meeting-detail__transcript">
        {transcript ? transcript : <em>전사 내용이 없습니다.</em>}
      </div>
    </div>
  );
}
