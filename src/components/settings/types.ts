import type { AppSettings } from "@/lib/types";

export interface SettingsActions {
  onPatch: (patch: Partial<AppSettings>) => void | Promise<void>;
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
  onSaveOpenRouterApiKey: (apiKey: string) => void;
  onDeleteOpenRouterApiKey: () => void;
  onSaveOpenAiApiKey: (apiKey: string) => void;
  onDeleteOpenAiApiKey: () => void;
}
