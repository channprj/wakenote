/**
 * Realistic sample content for the browser preview (`pnpm dev` outside Tauri).
 *
 * The browser mock in `tauri-client.ts` otherwise starts from an empty snapshot,
 * which makes the UI impossible to review or demo: every list is empty, every
 * model is `missing`, and no report exists to read. These fixtures seed a week of
 * captures, finished reports with real Markdown bodies, and one in-flight run so
 * every surface can be seen with content in it.
 *
 * Seeding is opt-in: only `main.tsx` calls `seedBrowserFixtures`, and only when
 * the app is running outside Tauri. Tests import `tauri-client` directly and are
 * unaffected.
 */
import type {
  LlmReportHistoryDetail,
  LlmReportRunSnapshot,
  ModelDescriptor,
  RecentTranscript,
  SettingsPatch,
} from "./types";
import { mockModels } from "./app-state";

export interface DevFixtures {
  settings: SettingsPatch;
  models: ModelDescriptor[];
  transcripts: RecentTranscript[];
  reports: LlmReportHistoryDetail[];
  runs: LlmReportRunSnapshot[];
  openrouterApiKey: string;
}

const saveRoot = "~/Documents/WakeNote";
const fixtureModel = "z-ai/glm-5.2";

/** A capture, expressed relative to the seeded day so fixtures never go stale. */
interface CaptureSeed {
  /** Days before today. 0 is today. */
  daysAgo: number;
  time: [hour: number, minute: number, second: number];
  text: string;
  source?: "microphone" | "system";
  sourceLabel?: string | null;
}

const captureSeeds: readonly CaptureSeed[] = [
  {
    daysAgo: 0,
    time: [9, 12, 4],
    text: "오늘 오전 스탠드업 시작할게요. 먼저 릴리스 브랜치 상태부터 공유드리겠습니다.",
  },
  {
    daysAgo: 0,
    time: [9, 13, 22],
    text: "리포트 생성 파이프라인은 어제 머지했고, 지금은 마크다운 렌더링만 남았습니다. 오늘 안에 끝낼 수 있을 것 같아요.",
  },
  {
    daysAgo: 0,
    time: [9, 15, 8],
    text: "Whisper medium 모델로 바꾼 뒤에 한국어 인식률이 눈에 띄게 좋아졌습니다. 다만 첫 단어가 잘리는 경우가 있어서 pre-roll 값을 600ms로 올렸어요.",
  },
  {
    daysAgo: 0,
    time: [9, 18, 41],
    text: "액션 아이템 정리하면, 저는 렌더링 마무리하고 지훈님은 온보딩 문구 검토, 수민님은 QA 시나리오 작성입니다.",
  },
  {
    daysAgo: 1,
    time: [14, 2, 11],
    text: "Design review for the reports page. The main complaint from last week's usability pass was that people could not find where to create a report.",
    source: "system",
    sourceLabel: "meet",
  },
  {
    daysAgo: 1,
    time: [14, 5, 47],
    text: "So the decision is: put a primary New report button on the Reports page itself, and keep the transcript selection flow as a shortcut. Both paths open the same dialog.",
    source: "system",
    sourceLabel: "meet",
  },
  {
    daysAgo: 1,
    time: [14, 9, 3],
    text: "빈 화면일 때 아무 안내가 없는 것도 문제예요. 처음 켠 사용자는 뭘 해야 할지 모릅니다. 빈 상태마다 다음 행동을 제안하는 컴포넌트를 하나 만들죠.",
    source: "system",
    sourceLabel: "meet",
  },
  {
    daysAgo: 2,
    time: [11, 30, 19],
    text: "고객 인터뷰 요약입니다. 회의를 녹음해두고 나중에 요약만 읽는 패턴이 가장 많았고, 전체 스크립트를 읽는 사람은 거의 없었습니다.",
  },
  {
    daysAgo: 2,
    time: [11, 34, 55],
    text: "That maps directly onto our two report kinds. The summary is the default read, and the detailed report is what you open when you need to verify something or write it up for someone else.",
  },
  {
    daysAgo: 3,
    time: [16, 45, 2],
    text: "성능 점검 결과 공유합니다. 트랜스크립트가 2천 건 넘어가면 목록 스크롤이 버벅이는데, content-visibility 적용으로 대부분 해결됐습니다.",
  },
  {
    daysAgo: 3,
    time: [16, 48, 30],
    text: "검색이 없어서 예전 녹음을 찾을 방법이 사실상 없습니다. 날짜를 정확히 기억해야만 찾을 수 있어요. 이건 우선순위 올려야 합니다.",
  },
  {
    daysAgo: 4,
    time: [10, 5, 13],
    text: "주간 기획 회의 시작합니다. 이번 주 목표는 캡처에서 보고서까지 한 번에 이어지는 흐름을 완성하는 것입니다.",
  },
  {
    daysAgo: 4,
    time: [10, 11, 27],
    text: "OpenRouter 비용도 봐야 합니다. 상세 보고서 한 번 생성에 3회 반복이 들어가니까 요약보다 3배 가까이 비싸요. 사용량을 화면에 보여주는 게 좋겠습니다.",
  },
];

const summaryReportBody = `# 오전 스탠드업 요약

**대상 기간** 2026-07-27 · **트랜스크립트** 4건

## 한 줄 요약

리포트 생성 파이프라인은 머지 완료됐고, 남은 작업은 마크다운 렌더링이다.

## 핵심 내용

- 릴리스 브랜치는 정상 상태이며 차단 이슈 없음
- Whisper medium 전환 후 한국어 인식률이 뚜렷하게 개선됨
- 첫 단어 잘림 현상 대응으로 pre-roll을 \`400ms\` → \`600ms\`로 상향

## 결정 사항

| 결정 | 배경 | 담당 |
| --- | --- | --- |
| pre-roll 600ms 유지 | 문장 앞부분 잘림 재발 방지 | 희찬 |
| 마크다운 렌더링 우선 처리 | 보고서 가독성이 제품 가치의 핵심 | 희찬 |

## 액션 아이템

- [x] 리포트 생성 파이프라인 머지
- [ ] 마크다운 렌더링 마무리 — 희찬
- [ ] 온보딩 문구 검토 — 지훈
- [ ] QA 시나리오 작성 — 수민

## 남은 질문

> pre-roll을 올리면서 늘어난 파일 크기가 장기 보관에 부담이 되는지는 아직 측정하지 않았다.
`;

const detailedReportBody = `# Reports 화면 개선 상세 보고서

**대상 기간** 2026-07-25 ~ 2026-07-26 · **트랜스크립트** 6건 · **모델** ${fixtureModel}

# Summary

지난 주 사용성 점검에서 나온 지적은 한 문장으로 요약된다. **사용자가 보고서를 만들 수 있는 곳을
찾지 못했다.** 생성 동작이 Transcripts 화면의 선택 툴바에만 있었기 때문이다. 이번 논의에서
Reports 화면 자체에 기본 생성 동작을 두고, 기존 경로는 바로가기로 유지하기로 결정했다.

# Context

WakeNote의 값은 캡처 → 트랜스크립트 → 요약/상세 보고서 → 읽기로 이어지는 사슬에서 나온다.
구현은 이미 이 사슬을 전부 지원하지만, 화면은 마지막 두 단계를 거의 드러내지 않았다.

- 보고서 본문이 \`<pre>\` 안의 원본 마크다운으로 출력됨
- 빈 화면에 다음 행동에 대한 안내가 없음
- 비파괴 숨김 기능이 본래 작업보다 화면 상단을 차지함

# Chronological Details

| 시각 | 출처 | 내용 |
| --- | --- | --- |
| 07-26 14:02 | Meet | 사용성 점검 결과 공유. 생성 위치를 못 찾는 문제 제기 |
| 07-26 14:05 | Meet | Reports 화면에 기본 생성 버튼 배치로 결정 |
| 07-26 14:09 | Meet | 빈 상태 안내 컴포넌트 신설 논의 |
| 07-25 11:30 | Mic | 고객 인터뷰: 요약만 읽는 패턴이 지배적 |
| 07-25 11:34 | Mic | 두 보고서 종류의 역할 정리 |

# Main Discussion Points

## 생성 진입점

두 경로가 같은 대화상자를 열도록 한다.

1. Reports 화면의 기본 **New report** 동작
2. Transcripts 화면의 선택 툴바 (바로가기로 유지)

## 두 보고서 종류의 역할

고객 인터뷰에서 관찰된 패턴이 기존 두 종류와 그대로 대응된다.

- **요약** — 기본으로 읽는 것. 회의 후 훑어보는 용도
- **상세 보고서** — 근거를 확인하거나 남에게 전달할 문서를 쓸 때 여는 것

## 비용

상세 보고서는 반복 3회가 기본이므로 요약보다 약 3배 비싸다. 화면에 사용량과 비용을 노출해
사용자가 선택 시점에 판단할 수 있어야 한다.

\`\`\`text
요약        1회 생성 + 1회 평가        ≈  4,800 tokens
상세 보고서  3회 생성 + 3회 평가        ≈ 15,200 tokens
\`\`\`

# Decisions

| # | 결정 | 근거 |
| --- | --- | --- |
| 1 | Reports 화면에 기본 생성 동작 배치 | 사용자가 찾는 위치가 그곳임 |
| 2 | 선택 툴바 경로 유지 | 이미 익숙한 사용자의 흐름을 깨지 않음 |
| 3 | 공용 빈 상태 컴포넌트 신설 | 모든 화면에서 같은 안내 품질 확보 |
| 4 | 숨김 툴바를 오버플로로 이동 | 부차 기능이 본 작업을 가리지 않도록 |

# Action Items

- [ ] Reports 화면 생성 대화상자 구현 — 희찬
- [ ] 빈 상태 컴포넌트 및 전면 적용 — 희찬
- [ ] 온보딩 문구 검토 — 지훈
- [ ] QA 시나리오 작성 — 수민
- [ ] 사용량/비용 표시 위치 결정 — 미정

# Risks and Issues

- 검색 기능이 없어 오래된 녹음을 찾을 실질적 방법이 없다. 아카이브는 계속 늘어나므로
  시간이 지날수록 악화된다.
- 트랜스크립트 2천 건 이상에서 스크롤 성능 저하가 관찰되었다. \`content-visibility\`로
  대부분 완화되었으나 상한은 아직 측정되지 않았다.

# Open Questions

> 상세 보고서의 반복 횟수를 사용자가 직접 조절하게 할 것인가, 아니면 품질 기준 충족까지
> 자동으로 맡길 것인가?

> pre-roll 상향으로 늘어난 저장 용량이 장기 보관에 부담이 되는가?

# Evidence Notes

이 보고서의 모든 항목은 아래 캡처에 근거한다. 추정이 필요한 곳은 본문에 명시했다.

- \`20260726/140211.m4a\` — 사용성 점검 결과
- \`20260726/140547.m4a\` — 생성 진입점 결정
- \`20260726/140903.m4a\` — 빈 상태 논의
- \`20260725/113019.m4a\` — 고객 인터뷰 요약
- \`20260725/113455.m4a\` — 보고서 종류 역할 정리
`;

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

function shiftDays(now: Date, daysAgo: number): Date {
  const date = new Date(now);
  date.setDate(date.getDate() - daysAgo);
  return date;
}

function captureDate(now: Date, seed: CaptureSeed): Date {
  const date = shiftDays(now, seed.daysAgo);
  date.setHours(seed.time[0], seed.time[1], seed.time[2], 0);
  return date;
}

function dayFolder(date: Date): string {
  return `${date.getFullYear()}${pad2(date.getMonth() + 1)}${pad2(date.getDate())}`;
}

function timeBasename(date: Date): string {
  return `${pad2(date.getHours())}${pad2(date.getMinutes())}${pad2(date.getSeconds())}`;
}

function transcriptFromSeed(now: Date, seed: CaptureSeed): RecentTranscript {
  const date = captureDate(now, seed);
  const folder = `${saveRoot}/${dayFolder(date)}`;
  const basename = timeBasename(date);
  return {
    transcript_path: `${folder}/${basename}.txt`,
    audio_path: `${folder}/${basename}.m4a`,
    recorded_at: date.toISOString(),
    text: seed.text,
    source: seed.source ?? "microphone",
    source_label: seed.sourceLabel ?? null,
  };
}

/** Marks the models a demo user would plausibly have on disk. */
function fixtureModels(): ModelDescriptor[] {
  const installed = new Set(["whisper-medium", "whisper-turbo", "sensevoice-small"]);
  return mockModels().map((model) =>
    installed.has(model.id) ? { ...model, status: "installed" } : model,
  );
}

function usage(promptTokens: number, completionTokens: number, requests: number) {
  return {
    request_count: requests,
    prompt_tokens: promptTokens,
    completion_tokens: completionTokens,
    total_tokens: promptTokens + completionTokens,
    cost: Number(((promptTokens + completionTokens) * 0.0000012).toFixed(6)),
  };
}

function fixtureReports(now: Date): LlmReportHistoryDetail[] {
  const summaryCreated = shiftDays(now, 0);
  summaryCreated.setHours(9, 24, 12, 0);
  const detailedCreated = shiftDays(now, 1);
  detailedCreated.setHours(15, 2, 38, 0);

  return [
    {
      item: {
        report_id: "fixture-summary-standup",
        kind: "summary",
        created_at: summaryCreated.toISOString(),
        file_name: `${dayFolder(summaryCreated)}-summary.md`,
        report_path: `${saveRoot}/reports/${dayFolder(summaryCreated)}-summary.md`,
        model: fixtureModel,
        iterations_used: 1,
        max_iterations: 3,
        success_criteria_met: true,
        completion_reason: "success_criteria_met",
        quality_feedback:
          "네 건의 발화를 모두 반영했고 근거 없는 추정이 없습니다. 액션 아이템의 담당자가 명확합니다.",
        selected_count: 4,
        date_range: formatDay(summaryCreated),
        usage: usage(3_180, 1_640, 2),
        legacy: false,
      },
      content: summaryReportBody,
    },
    {
      item: {
        report_id: "fixture-detailed-reports-page",
        kind: "detailed_report",
        created_at: detailedCreated.toISOString(),
        file_name: `${dayFolder(detailedCreated)}-detailed-report.md`,
        report_path: `${saveRoot}/reports/${dayFolder(detailedCreated)}-detailed-report.md`,
        model: fixtureModel,
        iterations_used: 2,
        max_iterations: 3,
        success_criteria_met: true,
        completion_reason: "success_criteria_met",
        quality_feedback:
          "2회차에서 근거 표와 본문의 시각이 어긋난 부분을 수정한 뒤 기준을 충족했습니다.",
        selected_count: 6,
        date_range: `${formatDay(shiftDays(now, 2))} ~ ${formatDay(shiftDays(now, 1))}`,
        usage: usage(11_420, 3_810, 5),
        legacy: false,
      },
      content: detailedReportBody,
    },
  ];
}

function formatDay(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

function fixtureRuns(
  now: Date,
  reports: readonly LlmReportHistoryDetail[],
): LlmReportRunSnapshot[] {
  const running = new Date(now);
  running.setMinutes(running.getMinutes() - 2, 0, 0);
  const failed = shiftDays(now, 2);
  failed.setHours(19, 41, 6, 0);

  const completedRuns = reports.map((report) => ({
    run_id: `fixture-run-${report.item.report_id}`,
    parent_run_id: null,
    revision: 4,
    status: "completed" as const,
    stage: "completed" as const,
    kind: report.item.kind,
    created_at: report.item.created_at,
    updated_at: report.item.created_at,
    started_at: report.item.created_at,
    finished_at: report.item.created_at,
    iteration: report.item.iterations_used ?? 1,
    max_iterations: report.item.max_iterations ?? 3,
    message: "Success criteria met",
    detail: report.item.quality_feedback,
    error: null,
    progress: [],
    model: fixtureModel,
    selected_count: report.item.selected_count ?? 0,
    date_range: report.item.date_range ?? "",
    report_id: report.item.report_id,
    report_path: report.item.report_path,
    completion_reason: "success_criteria_met" as const,
    success_criteria_met: true,
    quality_feedback: report.item.quality_feedback,
    usage: report.item.usage,
  }));

  const inFlightRunId = "fixture-run-in-flight";
  const failedRunId = "fixture-run-failed";

  const inFlight: LlmReportRunSnapshot = {
    run_id: inFlightRunId,
    parent_run_id: null,
    revision: 3,
    status: "running",
    stage: "refining",
    kind: "detailed_report",
    created_at: running.toISOString(),
    updated_at: running.toISOString(),
    started_at: running.toISOString(),
    finished_at: null,
    iteration: 2,
    max_iterations: 3,
    message: "근거 표와 본문의 시각 표기를 맞추는 중",
    detail: "1회차 평가에서 Chronological Details의 시각이 본문과 어긋난다는 지적이 있었습니다.",
    error: null,
    progress: [
      progressEvent(inFlightRunId, "preparing", "선택한 트랜스크립트를 정리했습니다", 0, 3),
      progressEvent(inFlightRunId, "generating", "1회차 초안을 작성했습니다", 1, 3),
      progressEvent(
        inFlightRunId,
        "evaluating",
        "1회차 초안을 품질 기준과 비교했습니다",
        1,
        3,
        "근거 표의 시각이 본문과 어긋납니다. 두 곳을 맞춰야 합니다.",
      ),
      progressEvent(inFlightRunId, "refining", "지적된 부분을 수정하고 있습니다", 2, 3),
    ],
    model: fixtureModel,
    selected_count: 5,
    date_range: formatDay(shiftDays(now, 3)),
    report_id: null,
    report_path: null,
    completion_reason: null,
    success_criteria_met: null,
    quality_feedback: null,
    usage: usage(8_240, 2_110, 4),
  };

  const failedRun: LlmReportRunSnapshot = {
    run_id: failedRunId,
    parent_run_id: null,
    revision: 2,
    status: "failed",
    stage: "failed",
    kind: "summary",
    created_at: failed.toISOString(),
    updated_at: failed.toISOString(),
    started_at: failed.toISOString(),
    finished_at: failed.toISOString(),
    iteration: 1,
    max_iterations: 3,
    message: "OpenRouter 요청이 실패했습니다",
    detail: null,
    error: "OpenRouter returned 429 Too Many Requests. Retry after 32s.",
    progress: [
      progressEvent(failedRunId, "preparing", "선택한 트랜스크립트를 정리했습니다", 0, 3),
      progressEvent(failedRunId, "generating", "1회차 초안을 요청했습니다", 1, 3),
      progressEvent(failedRunId, "failed", "OpenRouter 요청이 실패했습니다", 1, 3),
    ],
    model: fixtureModel,
    selected_count: 2,
    date_range: formatDay(failed),
    report_id: null,
    report_path: null,
    completion_reason: null,
    success_criteria_met: null,
    quality_feedback: null,
    usage: null,
  };

  return [inFlight, ...completedRuns, failedRun];
}

function progressEvent(
  runId: string,
  stage: LlmReportRunSnapshot["progress"][number]["stage"],
  message: string,
  iteration: number,
  maxIterations: number,
  detail?: string,
) {
  return {
    run_id: runId,
    stage,
    iteration,
    max_iterations: maxIterations,
    message,
    detail: detail ?? null,
  };
}

/** Sample content for the browser preview, anchored to `now` so it stays current. */
export function devFixtures(now: Date = new Date()): DevFixtures {
  const reports = fixtureReports(now);
  return {
    settings: {
      save_root: saveRoot,
      save_root_confirmed: true,
      calibration_completed: true,
      selected_model: "whisper-medium",
      openrouter_model: fixtureModel,
      threshold_dbfs: -44,
    },
    models: fixtureModels(),
    transcripts: captureSeeds.map((seed) => transcriptFromSeed(now, seed)),
    reports,
    runs: fixtureRuns(now, reports),
    openrouterApiKey: "sk-or-browser-preview",
  };
}
