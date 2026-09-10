import { FolderOpenIcon } from "lucide-react";
import { ModelManager } from "@/components/ModelManager";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { StatusBadge } from "@/components/ui/status-badge";
import { formatModelLabel } from "@/lib/models";
import { modelStatusBadgeTone } from "@/lib/status-summary";
import type { AppSnapshot } from "@/lib/types";
import { SettingsCard } from "./settings-controls";
import { TranscriptionOptionsSettings } from "./TranscriptionOptionsSettings";
import { TranscriptionCostDashboard } from "./TranscriptionCostDashboard";
import type { SettingsActions } from "./types";

export function ModelsSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings, models } = snapshot;
  const selectedStatus =
    models.find((model) => model.id === settings.selected_model)?.status ?? "";
  return (
    <div className="models-settings">
      <div data-slot="model-files-card">
        <SettingsCard
          title="Model files"
          description="Local speech recognition runtime."
        >
          <Field orientation="responsive" className="settings-row">
            <FieldContent>
              <FieldLabel>Model Directory</FieldLabel>
              <FieldDescription className="overflow-wrap-anywhere">
                Store downloaded model files in this folder.
              </FieldDescription>
            </FieldContent>
            <div className="settings-inline-control">
              <Input
                aria-label="Model Directory"
                value={settings.model_directory}
                title={settings.model_directory}
                onChange={(event) =>
                  actions.onPatch({
                    model_directory: event.currentTarget.value,
                  })
                }
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={actions.onChooseModelDirectory}
              >
                <FolderOpenIcon data-icon="inline-start" />
                Choose Folder
              </Button>
            </div>
          </Field>
          <Field orientation="horizontal" className="settings-row">
            <FieldContent>
              <FieldLabel>Selected model</FieldLabel>
              <FieldDescription>
                The active model used for new transcription jobs.
              </FieldDescription>
            </FieldContent>
            <StatusBadge tone={modelStatusBadgeTone(selectedStatus)}>
              {formatModelLabel(settings.selected_model, models)}
            </StatusBadge>
          </Field>
        </SettingsCard>
      </div>

      <TranscriptionOptionsSettings
        settings={settings}
        models={models}
        onPatch={actions.onPatch}
      />

      <TranscriptionCostDashboard
        onDetails={actions.onOpenTranscriptionCosts}
      />

      <ModelManager
        models={models}
        settings={settings}
        onPatch={actions.onPatch}
        onVerify={actions.onVerifyModel}
        onDownload={actions.onDownloadModel}
        onCancelDownload={actions.onCancelModelDownload}
        onDelete={actions.onDeleteModel}
      />
    </div>
  );
}
