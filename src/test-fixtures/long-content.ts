const koreanSeed =
  "레이아웃 경계를 검증하기 위한 긴 한국어 전사 문장과 괄호(상태)를 포함합니다. ";
const tokenSeed = "wake_note_identifier_without_any_break_opportunity_";

export const LONG_CONTENT = {
  korean: koreanSeed.repeat(6).slice(0, 240),
  token: tokenSeed.repeat(6).slice(0, 240),
  path: `/Volumes/990EVO+/workspace/chann/wakenote/${"nested/".repeat(30)}recording.m4a`,
  url: `https://example.com/transcripts?selection=${"very-long-value-".repeat(20)}`,
  model: `provider/${"long-model-version-".repeat(12)}latest`,
  error: `Runtime stream failed: ${"device-disconnected/".repeat(25)}`,
} as const;
