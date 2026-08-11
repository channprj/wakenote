import { useEffect, useMemo, useState } from "react";
import { Loader2Icon } from "lucide-react";
import { ModelSelectGroups } from "@/components/ModelSelectGroups";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { meetingTranscriptionModels } from "@/lib/models";
import type {
  MeetingTranscriptionRequest,
  ModelDescriptor,
  TranscriptionLanguage,
} from "@/lib/types";

export interface MeetingTranscriptionDialogProps {
  open: boolean;
  onOpenChange(open: boolean): void;
  models: ModelDescriptor[];
  initialModelId: string;
  initialLanguage: TranscriptionLanguage;
  targetCount: number;
  busy: boolean;
  error?: string | null;
  onSubmit(request: MeetingTranscriptionRequest): Promise<boolean> | boolean;
}

function resolveInitialModelId(
  initialModelId: string,
  models: ModelDescriptor[],
): string {
  return models.some((model) => model.id === initialModelId)
    ? initialModelId
    : (models[0]?.id ?? "");
}

export function MeetingTranscriptionDialog({
  open,
  onOpenChange,
  models,
  initialModelId,
  initialLanguage,
  targetCount,
  busy,
  error = null,
  onSubmit,
}: MeetingTranscriptionDialogProps) {
  const meetingModels = useMemo(
    () => meetingTranscriptionModels(models),
    [models],
  );
  const resolvedInitialModelId = resolveInitialModelId(
    initialModelId,
    meetingModels,
  );
  const [modelId, setModelId] = useState(resolvedInitialModelId);
  const [language, setLanguage] =
    useState<TranscriptionLanguage>(initialLanguage);
  const [streamingEnabled, setStreamingEnabled] = useState(false);
  const [speakerSeparationEnabled, setSpeakerSeparationEnabled] =
    useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    const initialModel = meetingModels.find(
      (model) => model.id === resolvedInitialModelId,
    );
    setModelId(resolvedInitialModelId);
    setLanguage(initialLanguage);
    setStreamingEnabled(
      initialModel?.capabilities.streaming === "required",
    );
    setSpeakerSeparationEnabled(false);
  }, [
    initialLanguage,
    meetingModels,
    open,
    resolvedInitialModelId,
  ]);

  const selectedModel = meetingModels.find((model) => model.id === modelId);
  const streamingRequired =
    selectedModel?.capabilities.streaming === "required";
  const streamingAvailable =
    selectedModel?.capabilities.streaming === "optional" ||
    streamingRequired;
  const speakerSeparationAvailable =
    selectedModel?.capabilities.diarization ?? false;
  const active = busy || submitting;

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

  const submit = async () => {
    if (!selectedModel || active) {
      return;
    }
    setSubmitting(true);
    try {
      const succeeded = await onSubmit({
        model_id: selectedModel.id,
        language,
        streaming_enabled: streamingRequired || streamingEnabled,
        speaker_separation_enabled:
          speakerSeparationAvailable && speakerSeparationEnabled,
      });
      if (succeeded) {
        onOpenChange(false);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const plural = targetCount !== 1;

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!active || nextOpen) {
          onOpenChange(nextOpen);
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {plural
              ? `Transcribe ${targetCount} meetings`
              : "Transcribe saved meeting"}
          </DialogTitle>
          <DialogDescription>
            {plural
              ? "These settings apply to every ready meeting in this selection. Each meeting keeps its own progress and transcript."
              : "Choose a meeting-capable model. Processing starts only after you confirm."}
          </DialogDescription>
        </DialogHeader>
        {error ? (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}
        <div className="meeting-transcription-options">
          <label>
            <span>Model</span>
            <Select
              value={modelId}
              onValueChange={changeModel}
              disabled={active}
            >
              <SelectTrigger
                size="sm"
                className="w-full"
                aria-label="Meeting transcription model"
              >
                <SelectValue placeholder="Choose a model" />
              </SelectTrigger>
              <SelectContent>
                <ModelSelectGroups models={meetingModels} />
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
              disabled={active}
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
              disabled={active || !streamingAvailable || streamingRequired}
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
              disabled={active || !speakerSeparationAvailable}
              onCheckedChange={setSpeakerSeparationEnabled}
            />
          </label>
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={active}
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            type="button"
            disabled={!selectedModel || active || targetCount === 0}
            onClick={() => void submit()}
          >
            {active ? (
              <Loader2Icon
                data-icon="inline-start"
                className="meeting-spin"
              />
            ) : null}
            {plural
              ? `Start ${targetCount} transcriptions`
              : "Start transcription"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
