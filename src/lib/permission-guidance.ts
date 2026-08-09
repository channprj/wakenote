import type { AppPermissions } from "./types";

export type PermissionKind =
  | "accessibility"
  | "microphone"
  | "screen_recording";

export type PermissionFeature =
  | "live_input"
  | "manual_meeting"
  | "dictation_recording"
  | "dictation_insertion"
  | "resume_source";

const FEATURE_PERMISSIONS = {
  live_input: ["microphone"],
  manual_meeting: ["microphone", "screen_recording"],
  dictation_recording: ["microphone"],
  dictation_insertion: ["accessibility"],
  resume_source: ["screen_recording"],
} as const satisfies Record<
  PermissionFeature,
  readonly PermissionKind[]
>;

const ONBOARDING_PERMISSIONS: readonly PermissionKind[] = [
  "accessibility",
  "microphone",
  "screen_recording",
];

export function permissionOnboardingNeedsGuidance(
  permissions: AppPermissions,
): boolean {
  return ONBOARDING_PERMISSIONS.some(
    (permission) => permissions[permission].status !== "granted",
  );
}

export function requiredPermissions(
  feature: PermissionFeature,
): readonly PermissionKind[] {
  return FEATURE_PERMISSIONS[feature];
}

export function missingPermissions(
  feature: PermissionFeature,
  permissions: AppPermissions,
): PermissionKind[] {
  return requiredPermissions(feature).filter(
    (permission) => permissions[permission].status !== "granted",
  );
}
