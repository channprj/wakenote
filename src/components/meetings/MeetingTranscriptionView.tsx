import { useMemo, useState } from "react";
import {
  ChevronLeftIcon,
  CopyIcon,
  EyeIcon,
  EyeOffIcon,
  FileAudioIcon,
  FolderOpenIcon,
  Loader2Icon,
  Mic2Icon,
  RotateCcwIcon,
  SquareIcon,
  UploadIcon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import {
  Card,
  CardContent,
  CardFooter,
} from "@/components/ui/card";
import {
  ListVisibilityToolbar,
  type ListVisibilityMode,
} from "@/components/ListVisibilityToolbar";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { StatusBadge } from "@/components/ui/status-badge";
import { Switch } from "@/components/ui/switch";
import {
  canResumeMeeting,
  formatClock,
  formatEta,
  meetingStatusLabel,
  meetingStatusTone,
  progressPercent,
} from "@/lib/meeting-progress";
import { modelSupportsContext } from "@/lib/models";
import type {
  MeetingDetail,
  MeetingTranscriptionRequest,
  ManualMeetingRecordingStatus,
  MeetingProgressPayload,
  MeetingSummary,
  ModelDescriptor,
  TranscriptionLanguage,
} from "@/lib/types";

export interface MeetingTranscriptionViewProps {
  active: MeetingSummary[];
  past: MeetingSummary[];
  selected: MeetingDetail | null;
  progressById: Record<string, MeetingProgressPayload>;
  liveTextById: Record<string, string>;
  busy: boolean;
  manualRecording: ManualMeetingRecordingStatus;
  manualRecordingBusy: boolean;
  models: ModelDescriptor[];
  transcriptionBusy: boolean;
  error: string | null;
  visibilityMode: ListVisibilityMode;
  visibleCount: number;
  hiddenCount: number;
  selectedMeetingIds: string[];
  visibilityMutating: boolean;
  visibilityStatus: string;
  onImport: () => void;
  onStartManualRecording: () => void;
  onStopManualRecording: () => void;
  onStartTranscription: (
    id: string,
    request: MeetingTranscriptionRequest,
  ) => void;
  onOpen: (id: string) => void;
  onBack: () => void;
  onCancel: (id: string) => void;
  onResume: (id: string) => void;
  onCopy: (text: string) => void;
  onOpenFolder: (audioPath: string) => void;
  onVisibilityModeChange: (mode: ListVisibilityMode) => void;
  onMeetingSelectionChange: (id: string, selected: boolean) => void;
  onSelectAllMeetings: () => void;
  onClearMeetingSelection: () => void;
  onApplyMeetingSelection: () => void;
  onSetMeetingHidden: (id: string, hidden: boolean) => void;
}

export function MeetingTranscriptionView(props: MeetingTranscriptionViewProps) {
  if (props.selected) {
    return (
      <MeetingDetailView
        key={props.selected.record.id}
        detail={props.selected}
        {...props}
      />
    );
  }

  return (
    <div data-slot="meeting-transcription-view" className="meeting-panel">
      <section
        className="meeting-mode"
        data-state={props.manualRecording.state}
        aria-labelledby="meeting-mode-title"
      >
        <div className="meeting-mode__content">
          <span className="meeting-mode__eyebrow">Continuous recording</span>
          <div className="meeting-mode__title-row">
            <h2 id="meeting-mode-title">Meeting Mode</h2>
            <StatusBadge
              tone={props.manualRecording.state === "recording" ? "danger" : "neutral"}
            >
              {props.manualRecording.state === "recording" ? "Recording" : "Off"}
            </StatusBadge>
          </div>
          <p>Microphone + System Audio</p>
          <div className="meeting-mode__metrics" aria-live="polite">
            {props.manualRecording.state === "recording" ? (
              <>
                <span>{formatClock(props.manualRecording.elapsed_ms)} elapsed</span>
                <span>{formatClock(props.manualRecording.remaining_ms)} remaining</span>
              </>
            ) : (
              <span>{formatClock(props.manualRecording.remaining_ms)} maximum</span>
            )}
          </div>
        </div>
        {props.manualRecording.state === "recording" ? (
          <Button
            type="button"
            size="sm"
            variant="destructive"
            disabled={props.manualRecordingBusy}
            aria-label="Stop and save meeting recording"
            onClick={props.onStopManualRecording}
          >
            {props.manualRecordingBusy ? (
              <Loader2Icon data-icon="inline-start" className="meeting-spin" />
            ) : (
              <SquareIcon data-icon="inline-start" />
            )}
            Stop & save
          </Button>
        ) : (
          <Button
            type="button"
            size="sm"
            disabled={props.manualRecordingBusy}
            aria-label="Start meeting recording"
            onClick={props.onStartManualRecording}
          >
            {props.manualRecordingBusy ? (
              <Loader2Icon data-icon="inline-start" className="meeting-spin" />
            ) : (
              <Mic2Icon data-icon="inline-start" />
            )}
            Start recording
          </Button>
        )}
      </section>

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
          Import an existing recording or transcribe a saved meeting when ready
        </span>
      </div>

      <ListVisibilityToolbar
        mode={props.visibilityMode}
        visibleCount={props.visibleCount}
        hiddenCount={props.hiddenCount}
        selectedCount={props.selectedMeetingIds.length}
        totalInMode={props.active.length + props.past.length}
        mutating={props.visibilityMutating}
        statusMessage={props.visibilityStatus}
        onModeChange={props.onVisibilityModeChange}
        onSelectAll={props.onSelectAllMeetings}
        onClearSelection={props.onClearMeetingSelection}
        onApplySelection={props.onApplyMeetingSelection}
      />

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
              selected={props.selectedMeetingIds.includes(meeting.id)}
              visibilityMode={props.visibilityMode}
              visibilityMutating={props.visibilityMutating}
              onOpen={() => props.onOpen(meeting.id)}
              onSelectionChange={(selected) =>
                props.onMeetingSelectionChange(meeting.id, selected)
              }
              onSetHidden={(hidden) =>
                props.onSetMeetingHidden(meeting.id, hidden)
              }
              onCancel={() => props.onCancel(meeting.id)}
            />
          ))}
        </section>
      ) : null}

      <section className="meeting-list" aria-labelledby="past-meetings-title">
        <h2 id="past-meetings-title" className="meeting-list__title">
          {props.visibilityMode === "hidden"
            ? "Hidden meetings"
            : "Past meetings"}
        </h2>
        {props.past.length === 0 ? (
          <EmptyState
            className="meeting-empty"
            icon={props.visibilityMode === "hidden" ? EyeOffIcon : FileAudioIcon}
            title={
              props.visibilityMode === "hidden"
                ? "No hidden meetings."
                : "No meetings transcribed yet."
            }
            description={
              props.visibilityMode === "hidden"
                ? "Hidden meetings keep every source file on disk."
                : "Import a long recording to keep it separate from short transcripts."
            }
          />
        ) : (
          <ul>
            {props.past.map((meeting) => (
              <li key={meeting.id} className="meeting-row">
                <Checkbox
                  checked={props.selectedMeetingIds.includes(meeting.id)}
                  disabled={props.visibilityMutating}
                  aria-label={`Select ${meeting.title}`}
                  onCheckedChange={(checked) =>
                    props.onMeetingSelectionChange(
                      meeting.id,
                      checked === true,
                    )
                  }
                />
                <button
                  type="button"
                  className="meeting-row__main"
                  onClick={() => props.onOpen(meeting.id)}
                >
                  <span className="meeting-row__content">
                    <span className="meeting-row__title" title={meeting.title}>
                      {meeting.title}
                    </span>
                    <span className="meeting-row__meta">
                      {formatClock(meeting.duration_ms)} · {formatDate(meeting.created_at)} · {meeting.model_id}
                    </span>
                  </span>
                </button>
                <StatusBadge tone={meetingStatusTone(meeting.status)}>
                  {meetingStatusLabel(meeting.status)}
                </StatusBadge>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={props.visibilityMutating}
                  aria-label={`${props.visibilityMode === "visible" ? "Hide" : "Restore"} ${meeting.title}`}
                  onClick={() =>
                    props.onSetMeetingHidden(
                      meeting.id,
                      props.visibilityMode === "visible",
                    )
                  }
                >
                  {props.visibilityMode === "visible" ? (
                    <EyeOffIcon aria-hidden="true" />
                  ) : (
                    <EyeIcon aria-hidden="true" />
                  )}
                  {props.visibilityMode === "visible" ? "Hide" : "Restore"}
                </Button>
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
  selected,
  visibilityMode,
  visibilityMutating,
  onOpen,
  onSelectionChange,
  onSetHidden,
  onCancel,
}: {
  meeting: MeetingSummary;
  live?: MeetingProgressPayload;
  previewText: string;
  selected: boolean;
  visibilityMode: ListVisibilityMode;
  visibilityMutating: boolean;
  onOpen: () => void;
  onSelectionChange: (selected: boolean) => void;
  onSetHidden: (hidden: boolean) => void;
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
      <div className="meeting-progress-row__head">
        <Checkbox
          checked={selected}
          disabled={visibilityMutating}
          aria-label={`Select ${meeting.title}`}
          onCheckedChange={(checked) =>
            onSelectionChange(checked === true)
          }
        />
        <button
          type="button"
          className="meeting-progress-row__title min-w-0 overflow-wrap-anywhere"
          title={meeting.title}
          onClick={onOpen}
        >
          <Loader2Icon data-icon="inline-start" className="meeting-spin" />
          {meeting.title}
        </button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={visibilityMutating}
          aria-label={`${visibilityMode === "visible" ? "Hide" : "Restore"} ${meeting.title}`}
          onClick={() => onSetHidden(visibilityMode === "visible")}
        >
          {visibilityMode === "visible" ? (
            <EyeOffIcon aria-hidden="true" />
          ) : (
            <EyeIcon aria-hidden="true" />
          )}
          {visibilityMode === "visible" ? "Hide" : "Restore"}
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
      </div>
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
  models,
  transcriptionBusy,
  onBack,
  onResume,
  onCopy,
  onOpenFolder,
  onStartTranscription,
  visibilityMode,
  visibilityMutating,
  visibilityStatus,
  onSetMeetingHidden,
}: MeetingTranscriptionViewProps & { detail: MeetingDetail }) {
  const { record, transcript, audio_path: audioPath } = detail;
  const meetingModels = useMemo(
    () =>
      models.filter(
        (model) =>
          modelSupportsContext(model, "meeting") &&
          model.capabilities.file_transcription &&
          (!model.offline || ["ready", "installed"].includes(model.status)),
      ),
    [models],
  );
  const initialModelId = meetingModels.some(
    (model) => model.id === record.model_id,
  )
    ? record.model_id
    : (meetingModels[0]?.id ?? "");
  const [dialogOpen, setDialogOpen] = useState(false);
  const [modelId, setModelId] = useState(initialModelId);
  const [language, setLanguage] =
    useState<TranscriptionLanguage>(record.language);
  const [streamingEnabled, setStreamingEnabled] = useState(false);
  const [speakerSeparationEnabled, setSpeakerSeparationEnabled] =
    useState(false);
  const selectedModel = meetingModels.find((model) => model.id === modelId);
  const streamingRequired = selectedModel?.capabilities.streaming === "required";
  const streamingAvailable =
    selectedModel?.capabilities.streaming === "optional" || streamingRequired;
  const speakerSeparationAvailable =
    selectedModel?.capabilities.diarization ?? false;

  const changeModel = (nextModelId: string) => {
    setModelId(nextModelId);
    const next = meetingModels.find((model) => model.id === nextModelId);
    if (next?.capabilities.streaming === "required") {
      setStreamingEnabled(true);
    } else if (next?.capabilities.streaming === "unsupported") {
      setStreamingEnabled(false);
    }
    if (!next?.capabilities.diarization) {
      setSpeakerSeparationEnabled(false);
    }
  };

  const submitTranscription = () => {
    if (!selectedModel) {
      return;
    }
    onStartTranscription(record.id, {
      model_id: selectedModel.id,
      language,
      streaming_enabled: streamingRequired || streamingEnabled,
      speaker_separation_enabled:
        speakerSeparationAvailable && speakerSeparationEnabled,
    });
    setDialogOpen(false);
  };

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
        {record.status === "recorded" ? (
          <Button
            type="button"
            size="sm"
            onClick={() => setDialogOpen(true)}
          >
            Configure transcription
          </Button>
        ) : null}
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
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={visibilityMutating}
          onClick={() =>
            onSetMeetingHidden(
              record.id,
              visibilityMode === "visible",
            )
          }
        >
          {visibilityMode === "visible" ? (
            <EyeOffIcon data-icon="inline-start" />
          ) : (
            <EyeIcon data-icon="inline-start" />
          )}
          {visibilityMode === "visible"
            ? "Hide from list"
            : "Restore to list"}
        </Button>
      </div>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Transcribe saved meeting</DialogTitle>
            <DialogDescription>
              Choose a meeting-capable model. Cloud processing starts only after
              you confirm this dialog.
            </DialogDescription>
          </DialogHeader>
          <div className="meeting-transcription-options">
            <label>
              <span>Model</span>
              <Select value={modelId} onValueChange={changeModel}>
                <SelectTrigger
                  size="sm"
                  className="w-full"
                  aria-label="Meeting transcription model"
                >
                  <SelectValue placeholder="Choose a model" />
                </SelectTrigger>
                <SelectContent>
                  {meetingModels.map((model) => (
                    <SelectItem key={model.id} value={model.id}>
                      {model.display_name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
            <label>
              <span>Language</span>
              <Select
                value={language}
                onValueChange={(value) =>
                  setLanguage(value as TranscriptionLanguage)
                }
              >
                <SelectTrigger
                  size="sm"
                  className="w-full"
                  aria-label="Meeting transcription language"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">Auto detect</SelectItem>
                  <SelectItem value="ko">Korean</SelectItem>
                  <SelectItem value="en">English</SelectItem>
                  <SelectItem value="ja">Japanese</SelectItem>
                  <SelectItem value="zh">Chinese</SelectItem>
                  <SelectItem value="es">Spanish</SelectItem>
                  <SelectItem value="fr">French</SelectItem>
                  <SelectItem value="de">German</SelectItem>
                </SelectContent>
              </Select>
            </label>
            <label className="meeting-transcription-option">
              <span>
                <strong>Streaming</strong>
                <small>
                  {streamingRequired
                    ? "Required by this model"
                    : streamingAvailable
                      ? "Receive partial results while each part is processed"
                      : "Not supported by this model"}
                </small>
              </span>
              <Switch
                size="sm"
                aria-label="Streaming"
                checked={streamingRequired || streamingEnabled}
                disabled={!streamingAvailable || streamingRequired}
                onCheckedChange={setStreamingEnabled}
              />
            </label>
            <label className="meeting-transcription-option">
              <span>
                <strong>Speaker separation</strong>
                <small>
                  {speakerSeparationAvailable
                    ? selectedModel?.offline
                      ? "Available on this on-device model"
                      : "Label speaker turns within each processed part"
                    : "Not supported by this model"}
                </small>
              </span>
              <Switch
                size="sm"
                aria-label="Speaker separation"
                checked={speakerSeparationEnabled}
                disabled={!speakerSeparationAvailable}
                onCheckedChange={setSpeakerSeparationEnabled}
              />
            </label>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setDialogOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              disabled={!selectedModel || transcriptionBusy}
              onClick={submitTranscription}
            >
              {transcriptionBusy ? (
                <Loader2Icon data-icon="inline-start" className="meeting-spin" />
              ) : null}
              Start transcription
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <span role="status" aria-live="polite" className="sr-only">
        {visibilityStatus}
      </span>

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
