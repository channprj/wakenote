import type { AppSettings } from "@/lib/types";
import type { PermissionFeature } from "@/lib/permission-guidance";

export interface SettingsActions {
  onPatch: (patch: Partial<AppSettings>) => void | Promise<void>;
  /** Explicit editors await persistence and display failures beside their draft. */
  onSavePatch?: (patch: Partial<AppSettings>) => Promise<void>;
  onPermissionRequired: (feature: PermissionFeature) => Promise<boolean>;
  onPreviewSubtitle: (patch: Partial<AppSettings>) => void | Promise<void>;
  onSetMicrophoneInputVolume: (
    deviceId: string,
    volumePercent: number,
  ) => void | Promise<void>;
  onSuspendDictationShortcut: () => void | Promise<void>;
  onResumeDictationShortcut: () => void | Promise<void>;
  onPressedModifierShortcut: () => Promise<string | null>;
  onChooseSaveRoot: () => void;
  onRevealSaveFolder: () => void;
  onChooseModelDirectory: () => void;
  onOpenDictionaryFile: () => void;
  onReloadDictionaryFile: () => void;
  onRequestAccessibilityPermission: () => void;
  onRequestMicrophonePermission: () => void;
  onRequestScreenRecordingPermission: () => void;
  onVerifyModel: (modelId: string) => void;
  onDownloadModel: (modelId: string) => void;
  onCancelModelDownload: (modelId: string) => void;
  onDeleteModel: (modelId: string) => void;
  onSaveOpenRouterApiKey: (apiKey: string) => void | Promise<void>;
  onDeleteOpenRouterApiKey: () => void | Promise<void>;
  onSaveOpenAiApiKey: (apiKey: string) => void | Promise<void>;
  onDeleteOpenAiApiKey: () => void | Promise<void>;
  onSaveSonioxApiKey: (apiKey: string) => void | Promise<void>;
  onDeleteSonioxApiKey: () => void | Promise<void>;
}
