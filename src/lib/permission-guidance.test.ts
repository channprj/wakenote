import { describe, expect, it } from "vitest";
import { defaultPermissions } from "./app-state";
import type { PermissionGrantStatus } from "./types";
import {
  missingPermissions,
  requiredPermissions,
  type PermissionFeature,
  type PermissionKind,
} from "./permission-guidance";

const EXPECTED_REQUIREMENTS: ReadonlyArray<
  readonly [PermissionFeature, readonly PermissionKind[]]
> = [
  ["live_input", ["microphone"]],
  ["manual_meeting", ["microphone", "screen_recording"]],
  ["dictation_recording", ["microphone"]],
  ["dictation_insertion", ["accessibility"]],
  ["resume_source", ["screen_recording"]],
];

describe("permission guidance", () => {
  it("maps each explicit action to its closed permission requirements", () => {
    for (const [feature, permissions] of EXPECTED_REQUIREMENTS) {
      expect(requiredPermissions(feature)).toEqual(permissions);
    }
  });

  it("treats every status except granted as unavailable", () => {
    const unavailable: PermissionGrantStatus[] = [
      "unknown",
      "not_determined",
      "denied",
      "restricted",
      "unsupported",
    ];

    for (const status of unavailable) {
      const permissions = defaultPermissions();
      permissions.microphone.status = status;
      expect(missingPermissions("live_input", permissions)).toEqual([
        "microphone",
      ]);
    }
  });

  it("checks only the permissions declared by the action", () => {
    const permissions = defaultPermissions();
    permissions.accessibility.status = "denied";
    permissions.screen_recording.status = "restricted";

    expect(missingPermissions("live_input", permissions)).toEqual([]);
    expect(missingPermissions("dictation_insertion", permissions)).toEqual([
      "accessibility",
    ]);
    expect(missingPermissions("manual_meeting", permissions)).toEqual([
      "screen_recording",
    ]);
  });
});
