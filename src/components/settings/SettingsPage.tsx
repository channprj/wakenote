import { useEffect, useRef } from "react";
import { PageHeader } from "@/components/shell/PageHeader";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { SETTINGS_SECTIONS, type SettingsSection } from "@/lib/navigation";
import type { AppSnapshot } from "@/lib/types";
import { AdvancedSettings } from "./AdvancedSettings";
import { AudioSettings } from "./AudioSettings";
import { DictationSettings } from "./DictationSettings";
import { GeneralSettings } from "./GeneralSettings";
import { IntegrationsSettings } from "./IntegrationsSettings";
import { ModelsSettings } from "./ModelsSettings";
import { StorageSettings } from "./StorageSettings";
import { SubtitlesSettings } from "./SubtitlesSettings";
import type { SettingsActions } from "./types";

export function SettingsPage({
  section,
  onSectionChange,
  snapshot,
  actions,
}: {
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const activeTabRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    activeTabRef.current?.scrollIntoView?.({
      block: "nearest",
      inline: "nearest",
    });
  }, [section]);

  return (
    <div data-slot="settings-page" className="settings-page">
      <PageHeader
        eyebrow="Application"
        title="Settings"
        description="Configure capture, dictation, subtitles, models, storage, and integrations."
      />
      <Tabs
        value={section}
        onValueChange={(value) => onSectionChange(value as SettingsSection)}
      >
        <div className="settings-tabs-scroll">
          <TabsList
            aria-label="Settings sections"
            className="max-w-full"
            variant="line"
          >
            {SETTINGS_SECTIONS.map((item) => (
              <TabsTrigger
                ref={section === item.id ? activeTabRef : undefined}
                key={item.id}
                value={item.id}
                id={`settings-tab-${item.id}`}
                aria-controls="settings-panel"
                className="shrink-0 px-2.5"
              >
                {item.label}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <div
          id="settings-panel"
          role="tabpanel"
          aria-labelledby={`settings-tab-${section}`}
          tabIndex={0}
          data-slot="settings-section"
          data-section={section}
          className="settings-section"
        >
          <SettingsSectionContent
            section={section}
            snapshot={snapshot}
            actions={actions}
          />
        </div>
      </Tabs>
    </div>
  );
}

export function SettingsSectionContent({
  section,
  snapshot,
  actions,
}: {
  section: SettingsSection;
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  switch (section) {
    case "general":
      return (
        <GeneralSettings
          settings={snapshot.settings}
          onPatch={actions.onPatch}
        />
      );
    case "audio":
      return <AudioSettings snapshot={snapshot} actions={actions} />;
    case "dictation":
      return <DictationSettings snapshot={snapshot} actions={actions} />;
    case "subtitles":
      return <SubtitlesSettings snapshot={snapshot} actions={actions} />;
    case "models":
      return <ModelsSettings snapshot={snapshot} actions={actions} />;
    case "storage":
      return <StorageSettings settings={snapshot.settings} actions={actions} />;
    case "integrations":
      return <IntegrationsSettings snapshot={snapshot} actions={actions} />;
    case "advanced":
      return (
        <AdvancedSettings
          settings={snapshot.settings}
          models={snapshot.models}
          onPatch={actions.onPatch}
          onSavePatch={actions.onSavePatch}
        />
      );
  }
}
