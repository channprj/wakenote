import {
  ChevronLeftIcon,
  CopyIcon,
  FileAudioIcon,
  FolderOpenIcon,
  Loader2Icon,
  RotateCcwIcon,
  Trash2Icon,
  UploadIcon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { StatusBadge } from "@/components/ui/status-badge";
import {
  canResumeMeeting,
  formatClock,
  formatEta,
  meetingStatusLabel,
  meetingStatusTone,
  progressPercent,
} from "@/lib/meeting-progress";
import type {
  MeetingDetail,
  MeetingProgressPayload,
  MeetingSummary,
} from "@/lib/types";

export interface MeetingTranscriptionViewProps {
  active: MeetingSummary[];
  past: MeetingSummary[];
  selected: MeetingDetail | null;
  progressById: Record<string, MeetingProgressPayload>;
  liveTextById: Record<string, string>;
  busy: boolean;
  error: string | null;
  onImport: () => void;
  onOpen: (id: string) => void;
  onBack: () => void;
  onCancel: (id: string) => void;
  onResume: (id: string) => void;
  onCopy: (text: string) => void;
  onOpenFolder: (audioPath: string) => void;
  onDelete: (id: string) => void;
}

export function MeetingTranscriptionView(props: MeetingTranscriptionViewProps) {
  if (props.selected) {
    return <MeetingDetailView detail={props.selected} {...props} />;
  }

  return (
    <div data-slot="meeting-transcription-view" className="meeting-panel">
      <div className="meeting-panel__toolbar">
        <Button type="button" size="sm" disabled={props.busy} onClick={props.onImport}>
          {props.busy ? (
            <Loader2Icon data-icon="inline-start" className="meeting-spin" />
          ) : (
            <UploadIcon data-icon="inline-start" />
          )}
          Import meeting audio
        </Button>
        <span className="meeting-panel__hint">
          Long recordings are transcribed segment by segment · No speaker separation
        </span>
      </div>

      {props.error ? (
        <Alert variant="destructive">
          <FileAudioIcon />
          <AlertTitle>Meeting action failed</AlertTitle>
          <AlertDescription>{props.error}</AlertDescription>
        </Alert>
      ) : null}

      {props.active.length > 0 ? (
        <section className="meeting-active-list" aria-labelledby="active-meetings-title">
          <h2 id="active-meetings-title">Active</h2>
          {props.active.map((meeting) => (
            <MeetingProgressRow
              key={meeting.id}
              meeting={meeting}
              live={props.progressById[meeting.id]}
              previewText={props.liveTextById[meeting.id] ?? ""}
              onCancel={() => props.onCancel(meeting.id)}
            />
          ))}
        </section>
      ) : null}

      <section className="meeting-list" aria-labelledby="past-meetings-title">
        <h2 id="past-meetings-title" className="meeting-list__title">Past meetings</h2>
        {props.past.length === 0 ? (
          <div className="meeting-empty">
            <FileAudioIcon aria-hidden="true" />
            <span>No meetings transcribed yet.</span>
            <small>Import a long recording to keep it separate from short transcripts.</small>
          </div>
        ) : (
          <ul>
            {props.past.map((meeting) => (
              <li key={meeting.id}>
                <button
                  type="button"
                  className="meeting-row"
                  onClick={() => props.onOpen(meeting.id)}
                >
                  <span className="meeting-row__main">
                    <span className="meeting-row__title" title={meeting.title}>
                      {meeting.title}
                    </span>
                    <span className="meeting-row__meta">
                      {formatClock(meeting.duration_ms)} · {formatDate(meeting.created_at)} · {meeting.model_id}
                    </span>
                  </span>
                  <StatusBadge tone={meetingStatusTone(meeting.status)}>
                    {meetingStatusLabel(meeting.status)}
                  </StatusBadge>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function MeetingProgressRow({
  meeting,
  live,
  previewText,
  onCancel,
}: {
  meeting: MeetingSummary;
  live?: MeetingProgressPayload;
  previewText: string;
  onCancel: () => void;
}) {
  const processed = live?.processed_ms ?? meeting.progress.processed_ms;
  const duration = live?.duration_ms ?? meeting.duration_ms;
  const segmentsDone = live?.segments_done ?? meeting.progress.segments_done;
  const segmentsTotal = live?.segments_total ?? meeting.progress.segments_total;
  const elapsed = live?.elapsed_ms ?? meeting.progress.elapsed_ms;
  const remaining = live?.eta_ms ?? 0;
  const percent = progressPercent(processed, duration);

  return (
    <Card size="sm" className="meeting-progress-row">
      <CardHeader>
        <CardTitle
          className="min-w-0 overflow-wrap-anywhere"
          title={meeting.title}
        >
          <Loader2Icon data-icon="inline-start" className="meeting-spin" />
          {meeting.title}
        </CardTitle>
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
      </CardHeader>
      <CardContent>
        <Progress value={percent} aria-label={`${percent}% complete`} />
        <div className="meeting-progress-row__meta">
          <span>{percent}%</span>
          <span>Segment {segmentsDone}/{segmentsTotal || "?"}</span>
          <span>Elapsed {formatClock(elapsed)}</span>
          <span>Remaining {formatEta(remaining)}</span>
        </div>
      </CardContent>
      <CardFooter>
        <span className="meeting-progress-row__preview" title={previewText || undefined}>
          {previewText || "Transcribing the first segment…"}
        </span>
      </CardFooter>
    </Card>
  );
}

function MeetingDetailView({
  detail,
  error,
  onBack,
  onResume,
  onCopy,
  onOpenFolder,
  onDelete,
}: MeetingTranscriptionViewProps & { detail: MeetingDetail }) {
  const { record, transcript, audio_path: audioPath } = detail;

  return (
    <div data-slot="meeting-detail" className="meeting-detail">
      <div className="meeting-detail__head">
        <Button type="button" size="sm" variant="ghost" onClick={onBack}>
          <ChevronLeftIcon data-icon="inline-start" />
          Meetings
        </Button>
        <span className="meeting-detail__title" title={record.title}>{record.title}</span>
        <StatusBadge tone={meetingStatusTone(record.status)}>
          {meetingStatusLabel(record.status)}
        </StatusBadge>
      </div>

      <div className="meeting-detail__meta">
        <span>{formatClock(record.duration_ms)}</span>
        <span title={record.model_id}>{record.model_id}</span>
        <span>{formatDate(record.created_at)}</span>
        <span>Segment {record.progress.segments_done}/{record.progress.segments_total}</span>
      </div>

      {error || record.error ? (
        <Alert variant="destructive">
          <FileAudioIcon />
          <AlertTitle>Meeting transcription interrupted</AlertTitle>
          <AlertDescription>{error ?? record.error}</AlertDescription>
        </Alert>
      ) : null}

      <div className="meeting-detail__actions">
        {canResumeMeeting(record.status) ? (
          <Button type="button" size="sm" variant="outline" onClick={() => onResume(record.id)}>
            <RotateCcwIcon data-icon="inline-start" />
            Resume
          </Button>
        ) : null}
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={!transcript}
          onClick={() => onCopy(transcript)}
        >
          <CopyIcon data-icon="inline-start" />
          Copy
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={() => onOpenFolder(audioPath)}>
          <FolderOpenIcon data-icon="inline-start" />
          Open Folder
        </Button>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button type="button" size="sm" variant="ghost">
              <Trash2Icon data-icon="inline-start" />
              Delete
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent size="sm">
            <AlertDialogHeader>
              <AlertDialogTitle>Delete this meeting?</AlertDialogTitle>
              <AlertDialogDescription>
                The imported audio, progress, and transcript for “{record.title}” will be removed.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Keep meeting</AlertDialogCancel>
              <AlertDialogAction variant="destructive" onClick={() => onDelete(record.id)}>
                Delete meeting
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>

      <article className="meeting-detail__transcript">
        {transcript || <em>No transcript content.</em>}
      </article>
    </div>
  );
}

function formatDate(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}
