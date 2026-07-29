# WakeNote

[English README](./README.md)

**macOS 전용 음성 활성화 로컬 transcription 앱.**

WakeNote은 메뉴바 앱입니다. 필수 Primary 마이크 한 대와 선택 Secondary 마이크 한 대를 모니터링하다가 입력이 dBFS 임계값을 넘으면 녹음 청크를 시작하고, 청크가 끝나면 로컬 Whisper로 transcription을 수행합니다. 마이크 두 대를 구성하면 물리 캡처 스트림은 서로 독립적으로 유지하면서 기본적으로 시간축을 맞춘 하나의 녹음·전사 입력으로 병합합니다. 오디오, 텍스트, 메타데이터는 모두 같은 폴더(날짜별 디렉터리) 안에 나란히 저장되어 노트·스크립트·백업 도구로 바로 grep할 수 있습니다.

기술 스택은 Tauri 2 (Rust 백엔드) + React 19 + TypeScript + Tailwind CSS v4입니다. Transcription은 macOS 빌드에서 Metal GPU 가속이 켜진 `whisper-rs` (whisper.cpp)로 오프라인 추론하며, 오디오 캡처는 `cpal`을 사용합니다. 기본 테마 색상은 블랙 `#000`입니다.

## 핵심 특징

- **음성 활성화 캡처** — RMS dBFS가 임계값 위로 *attack* 시간 이상 유지되어야 녹음이 시작되고, 임계값 아래로 *release* 시간 이상 유지되어야 종료됩니다. pre-roll / post-roll 버퍼로 발화의 시작과 끝이 잘리지 않게 보존합니다.
- **견고한 듀얼 마이크** — 설정에서 Primary 한 대와 선택 Secondary 한 대를 지정합니다. 각 마이크는 스트림, 프레임 큐, 레벨, 경고, 동일 장치 재연결 루프를 따로 가지므로 한쪽 장애가 다른 쪽을 중지하지 않습니다. 마이크가 두 대면 **Merge microphone inputs**가 기본으로 켜져 녹음 하나와 전사 하나를 만들며, 끄면 마이크별 녹음과 전사 식별자를 분리하는 기존 동작을 유지합니다. 입력 모니터링은 Primary에만 적용됩니다.
- **단축키 받아쓰기** — 선택 기능을 켜면 설정한 전역 단축키를 누르고 있는 동안 녹음하고 손을 떼면 로컬 전사한 뒤, 포커스된 커서에 최종 결과를 입력합니다. 받아쓰기 언어는 별도의 자동 감지를 지원하며 오디오는 보관함에 추가하지 않습니다.
- **녹음 / transcription / 일시정지 토글 분리** — 텍스트 없이 오디오만 저장, 신규 녹음 없이 기존 backlog만 transcription, 또는 트레이에서 전체 일시정지 가능.
- **로컬 우선 저장** — `{save_root}/YYYYMMDD/HHMMSS.{m4a|wav}` 오디오, `.txt` 전사, `.json` 메타데이터, 복구 가능한 transcription 오류는 `.error.txt`. 파일명이 충돌하면 `-001`, `-002` 식으로 자동 롤오버.
- **모델 매니저** — UI에서 모델을 다운로드 / 검증(SHA-256) / 취소 / 삭제 / 전환할 수 있습니다. 한국어 사용 가능한 기본 Whisper 레지스트리는 `whisper-small`, `whisper-medium`, `whisper-turbo`, `whisper-large`를 제공합니다. Parakeet V3, SenseVoice는 내장 sherpa-onnx 엔진으로 외부 도구 없이 온디바이스로 다운로드·실행되고, Nemotron 3.5 ASR은 external-command adapter로 실행됩니다.
- **AI 요약 / 상세 보고서** — 원하는 캡처를 골라 OpenRouter로 Markdown 문서를 만듭니다. 초안을 쓰고, 스스로 품질 기준에 맞춰 평가하고, 기준을 충족하거나 반복 예산이 끝날 때까지 다듬습니다. [AI 요약과 보고서](#ai-요약과-보고서) 참고.
- **단일 실행 transcription queue** — 동시에 한 작업만 실행. 실패한 작업은 복구 가능한 오류로 표시되고 retry / skip 가능. 이전 세션에서 running 상태였던 작업은 시작 시 pending으로 자동 복구됩니다.
- **견고한 라이브 캡처** — 오디오 콜백은 프레임을 bounded 백그라운드 큐에 넘깁니다. 처리가 입력 속도를 못 따라가면 오래된 프레임을 drop하고 입력 스레드를 막지 않으며, UI에는 runtime warning을 띄웁니다.
- **macOS 트레이 + 자막 overlay** — 트레이 아이콘이 상태(Idle / Listening / Recording / Transcribing / Paused / Error)를 색으로 보여주며, 빠른 토글과 `Reveal Save Folder` 액션을 제공합니다. Floating overlay는 현재 데스크톱에만 뜨는 click-through 자막 surface로, WakeNote 메인 창을 열지 않고 현재 live/final transcript text만 보여줍니다.
- **첫 실행 onboarding** — 마이크, 저장 폴더, 모델, 캘리브레이션 단계는 각각 명시적으로 확인되어야 complete 처리됩니다. 캘리브레이션이 끝나면 라이브 신호 기반 suggested threshold가 자동 적용됩니다.

## 동작 흐름

```
  ┌──────────────────────────────────────────────────────────────────┐
  │ macOS mics ─► 독립 cpal streams ─► MicrophoneMixer ────────────► │
  │                                   (두 입력, 기본 켜짐)           │
  │                                                   SpeechGate ─► │
  │                                                          │       │
  │                              ┌───────────────────────────┘       │
  │                              ▼                                   │
  │           pre-roll buffer ─► CaptureController ─► .m4a / .wav    │
  │                                          │       .json metadata  │
  │                                          ▼                       │
  │                       단일 실행 TranscriptionQueue ──► .txt       │
  │                                          │              .error.txt│
  │                                          ▼                       │
  │                       whisper-rs (로컬 Whisper 추론)              │
  └──────────────────────────────────────────────────────────────────┘
```

- `src-tauri/src/audio.rs` — `SpeechGate` (attack / release / pre-roll / post-roll / min·max chunk), `LevelMonitor` (current / peak dBFS, noise floor, suggested threshold).
- `src-tauri/src/capture.rs` — `CaptureController` / `CaptureProcessor`가 raw 프레임을 완성된 `RecordedChunk`로 변환.
- `src-tauri/src/live_capture.rs` — `LiveCaptureRuntime` + `CpalAudioInput`. bounded `FrameDispatcher`가 back-pressure 상황에서 stale 프레임을 drop.
- `src-tauri/src/multi_capture.rs` — Primary / Secondary 런타임을 서로 다른 스트림과 dispatcher로 조정하고, 병합을 켜면 프레임의 시간축과 sample rate를 맞춰 하나의 입력으로 만듭니다.
- `src-tauri/src/recorder.rs` — `hound`로 `.wav` 작성, macOS `afconvert`로 `.m4a` 작성 (PCM → WAV → AAC/M4A), `.json` 메타데이터 및 transcript / error sidecar.
- `src-tauri/src/queue.rs` — `TranscriptionQueue`, idempotent enqueue, 단일 실행 `start_next`, retry / skip / cancel.
- `src-tauri/src/transcription.rs` — `WhisperTranscriber` + `TranscriptionWorker`.
- `src-tauri/src/dictation.rs` — 단축키 검증, push-to-talk 상태, 독립 마이크 캡처, 임시 16 kHz 전사 입력, 최종 결과의 포커스된 커서 입력.
- `src-tauri/src/models.rs` — 모델 레지스트리, 진행률 / 취소 / 체크섬 검증을 포함한 다운로드, 디스크 `ModelStore`.
- `src-tauri/src/persistence.rs` — app data 디렉터리 아래의 `settings.json`과 `transcription-queue.json`을 atomic하게 저장. 시작 시 in-flight 작업을 pending으로 복구.
- `src/App.tsx`, `src/components/*` — 설정 UI, onboarding strip, level meter, queue panel, 모델 매니저, 트레이 프리뷰.
- `src/overlay/*` — click-through live caption window용 별도 Tauri overlay entrypoint.

## 출력 파일 구조

마이크 두 대를 켜면 기본 병합 모드는 `142301-mic-merged.m4a` 같은 파일 하나와
전사 작업 하나를 만들고, JSON의 `microphone_inputs`에 구성된 장치 두 대를
기록합니다. **Merge microphone inputs**를 끄면
`142301-mic-primary-wired.m4a`, `142301-mic-secondary-wireless.m4a`처럼
마이크별 파일이 생깁니다. 이때 JSON에는 `device_id`, `device_name`,
`microphone_slot`이 기록되고, Transcripts 화면은 두 입력을 시간순으로 섞어
실제 장치 이름 badge와 filter로 구분합니다. `System Default`는 마이크 한 대
구성에서만 사용할 수 있고, 두 대 구성은 서로 다른 명시적 물리 장치 두 대를
선택해야 합니다.

```
~/Documents/WakeNote/
└── 20260509/
    ├── 142301.m4a          # 오디오 (또는 .wav)
    ├── 142301.json         # ChunkMetadata: model, device, sample_rate,
    │                       # threshold, started_at, ended_at, duration_ms,
    │                       # transcription_status, app_version, …
    ├── 142301.txt          # 성공한 전사
    └── 142301.error.txt    # 복구 가능한 transcription 오류 (있을 때만)
```

## 기본 캡처 설정

| 항목 | 기본값 | 허용 범위 |
| --- | --- | --- |
| Threshold | `-40 dBFS` | `-90 … -10` |
| Mic input volume | `100%` | `0 … 200` |
| Attack | `200 ms` | `50 … 2 000` |
| Release | `1 000 ms` | `250 … 5 000` |
| Pre-roll | `400 ms` | `0 … 1 500` |
| Lead-in | `200 ms` | `0 … 2 000` |
| Post-roll | `400 ms` | `0 … 2 000` |
| Min chunk | `800 ms` | `100 … 5 000` |
| Max chunk | `180 000 ms` (3분) | `10 000 … 900 000` |
| 오디오 포맷 | `m4a` | `m4a` / `wav` |
| 마이크 입력 병합 | `on` | `on` / `off` |
| 저장 루트 | `~/Documents/WakeNote` | 임의 디렉터리 |
| 기본 모델 | `whisper-medium` | 레지스트리 내 모델 |
| 모델 디렉터리 | `~/Library/Application Support/WakeNote/models` | 임의 디렉터리 |
| Transcription 언어 | `ko` | `auto`, `ko`, `en`, `ja`, `zh`, `es`, `fr`, `de` |
| 단축키 받아쓰기 | `off` | `on` / `off` |
| 받아쓰기 단축키 | `Option+Space` | 보조 키 2개, 보조 키 + 지원 키 또는 `F1` … `F24` |
| 받아쓰기 언어 | `auto` | `auto`, `ko`, `en`, `ja`, `zh`, `es`, `fr`, `de` |
| 저신뢰 transcript 숨기기 | `on` | `on` / `off` |
| 실행 시 입력 자동 시작 | `on` | `on` / `off` |

설정은 `<app_data_dir>/settings.json`에 저장되며, patch가 적용될 때마다 안전 범위로 clamp됩니다.

## 단축키 받아쓰기

**Settings › Dictation › Shortcut dictation**을 켜고 단축키와 언어를 고른
다음, 텍스트를 입력할 앱에 커서를 둡니다.

1. 단축키를 누르고 있으면 시작 chirp가 끝난 뒤 설정된 Primary 마이크
   녹음이 시작됩니다.
2. 단축키를 누른 채 말하고 손을 뗍니다. 캡처를 먼저 닫은 뒤 종료 chirp와
   로컬 전사를 실행합니다.
3. Floating overlay가 꺼져 있어도 필수 상단 중앙 피드백을 계속 표시하고,
   비어 있지 않은 최종 결과만 포커스된 커서에 입력한 뒤 기존 클립보드
   내용을 복원합니다.

받아쓰기는 기본적으로 꺼져 있습니다. 언어 기본값은 **Auto-detect**이며
보관용 transcription 언어와 서로 독립적입니다. 받아쓰기 캡처는 별도
스트림을 사용해 음성 활성화 녹음을 중단하지 않으며, 임시 16 kHz WAV는
성공·실패와 관계없이 삭제됩니다. 신호가 너무 작으면 입력하지 않고,
전사 중 누른 단축키는 무시하며, 녹음은 10분 뒤 자동으로 종료됩니다.
`Control+Shift`처럼 보조 키 2개만 사용하는 조합도 push-to-talk로
지원하며, 누르면 캡처를 시작하고 손을 떼면 중지합니다.

macOS에서 WakeNote의 마이크 접근을 허용해야 합니다. 포커스된 커서에
입력하려면 **시스템 설정 › 개인정보 보호 및 보안 › 손쉬운 사용**에서
System Events를 통한 입력 권한도 허용해야 합니다.

## AI 요약과 보고서

전사는 절반일 뿐입니다. 회의를 녹음하는 이유는 결국 그 회의가 무엇으로 정리됐는지
나중에 읽기 위해서입니다. WakeNote는 캡처를 OpenRouter를 통해 Markdown 문서로 만듭니다.

### 화면 구성

| 화면 | 용도 |
| --- | --- |
| **Capture** | 실시간 녹음, 레벨, 캘리브레이션, 최근 인식된 문장. |
| **Transcripts** | 날짜별 짧은 캡처 보관함. 선택해서 바로 보고서를 만들 수 있습니다. |
| **Meetings** | 가져온 장시간 녹음. 세그먼트 단위로 전사합니다. |
| **Reports** | 보고서를 만들고 읽는 곳. |
| **Activity** | 전사 큐 — 대기, 실행, 완료, 실패. |

### 두 가지 보고서

| 종류 | 구성 | 비용 |
| --- | --- | --- |
| **요약** | 한 줄 요약, 핵심 내용, 결정 사항, 액션 아이템, 남은 질문. | 초안 1회 + 평가 1회. |
| **상세 보고서** | 여기에 맥락, 시간순 상세, 리스크, 개별 캡처로 되짚을 수 있는 근거 노트를 더합니다. | 최대 `llm_max_iterations`회 초안/평가 반복. 기본값에서 요약의 약 3배. |

두 프롬프트 모두 **Settings › Integrations**에서 편집할 수 있는 템플릿
(`llm_summary_prompt_template`, `llm_report_prompt_template`)이며,
`{{transcripts}}`, `{{date_range}}`, `{{selected_count}}` 플레이스홀더를 씁니다.
둘 다 주어진 전사만 근거로 쓰고, 전사의 주 언어로 작성하도록 지시합니다.

### 만들기

진입점은 두 개, 대화상자는 하나입니다.

- **Reports › New report** — 종류를 고르고 어느 날짜의 캡처를 담을지 선택합니다.
  대화상자가 대상 범위, 모델, 반복 예산을 먼저 알려주므로 쓰기 전에 판단할 수 있습니다.
- **Transcripts에서 캡처 선택 → Summary / Report** — 이미 원하는 캡처를 보고 있을 때의 바로가기.

생성에는 OpenRouter API 키가 필요합니다(**Settings › Integrations**). 보고서는 한 번에
하나만 실행되며, 진행 중인 작업은 중단할 수 있고 취소·실패한 작업은 다시 시도할 수
있습니다. 진행 상황은 preparing / generating / evaluating / refining / saving 단계로
보여주고, 평가 단계의 피드백도 그때그때 확인할 수 있습니다.

### 읽기

보고서는 원본 Markdown이 아니라 문서로 렌더링됩니다. 제목 계층, GFM 표, 체크리스트,
코드 블록, 인용을 모두 표시합니다. 각 보고서는 사용한 모델, 소요 반복 횟수, 담은 캡처 수,
대상 기간, 토큰 수, OpenRouter 비용과 마지막 품질 피드백을 함께 보여줍니다. 여기서
Markdown을 **Copy**하거나 **Download Markdown**으로 저장하거나 **Run again**으로 다시
생성할 수 있습니다.

보고서 본문은 `{save_root}/reports/`에 저장되고 디스크에 남습니다. 보고서를 숨기는 것은
목록에서만 감추는 동작이며, 파일은 삭제되지 않습니다.

## 추가 ASR provider

WakeNote 기본 레지스트리에는 `parakeet-tdt-0.6b-v3`, `sensevoice-small`, `nemotron-3.5-asr-streaming-0.6b` 항목이 포함됩니다.

### Parakeet V3 / SenseVoice (온디바이스, 외부 도구 불필요)

두 모델은 **앱에 내장된 [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)(onnxruntime) 엔진으로 in-process 실행**됩니다 — 내장 whisper.cpp 엔진처럼 외부 CLI나 Python 설치가 전혀 필요 없습니다. 모델 목록에서 **Download**를 누르면 WakeNote가 sherpa-onnx 릴리스의 공식 ONNX 아카이브를 받아 모델 디렉터리에 압축 해제하고, 모델이 Ready가 되어 선택할 수 있습니다. `Delete`는 압축 해제된 모델을 제거합니다.

엔진은 `asr-sherpa` Cargo feature로 빌드됩니다. 배포 빌드(`pnpm build`)는 이를 자동으로 켜며, 일반 `cargo build`/`cargo test`는 가볍게 유지하려고 이를 제외합니다(이 경우 해당 모델 선택 시 "asr-sherpa feature" 오류를 반환). prebuilt onnxruntime + sherpa-onnx dylib은 빌드 시 내려받아 앱의 `Contents/Frameworks`에 번들로 포함되며(바이너리에 `@executable_path/../Frameworks` rpath 추가), 따라서 배포된 `.app`은 자체 완결적입니다. 정적 링크를 원하면 sherpa-rs의 `static` feature로 바꾸면 됩니다.

### Nemotron 3.5 ASR (external command)

NVIDIA는 Nemotron 3.5 ASR을 NeMo 체크포인트로만 배포하고 ONNX export가 없어 내장 sherpa-onnx 엔진으로는 돌릴 수 없습니다. 대신 external-command adapter로 실행합니다: 원하는 NeMo runner를 설치한 뒤 `<model_directory>/nemotron-3.5-asr-streaming-0.6b.command` 파일로 연결하세요. 파일이 존재하면 WakeNote가 모델을 Ready로 표시하며, command는 `WAKENOTE_AUDIO_PATH`, `WAKENOTE_MODEL_ID`, `WAKENOTE_MODEL_DIRECTORY`, `WAKENOTE_LANGUAGE` 환경변수를 읽고 transcript를 stdout으로 출력합니다.

### 커스텀 external-command 모델 (고급)

다른 엔진을 붙이려면 `<model_directory>/<model-id>.command` 파일에 실행할 shell command를 넣어두면 됩니다. 파일이 존재하면 WakeNote가 해당 모델을 ready로 표시합니다. command는 `WAKENOTE_AUDIO_PATH`, `WAKENOTE_MODEL_ID`, `WAKENOTE_MODEL_DIRECTORY`, `WAKENOTE_LANGUAGE` 환경변수를 읽고 transcript를 stdout으로 출력해야 합니다.

## 시스템 요구사항

- macOS 11.0 (Big Sur) 이상 — 배포 앱은 arm64 빌드이고 onnxruntime를 번들하므로 둘 다 11.0을 요구합니다.
- Node.js 20+ 와 `pnpm` 10 (`packageManager` 필드로 `pnpm@10.33.4` 고정).
- Rust toolchain (stable, `edition = "2024"`).
- Xcode Command Line Tools — `afconvert` (M4A 인코딩) 및 Tauri 빌드 체인에 필요.
- 첫 실행 시 마이크 권한 허용 필요.

## 시작하기

```bash
# 1. JS 의존성 설치
pnpm install

# 2. 데스크톱 앱 개발 모드 실행 (Vite + Tauri)
pnpm tauri dev

# 3. 프론트엔드만 실행 (mock 백엔드의 브라우저 dev fallback)
pnpm dev

# 4. 릴리스 번들 빌드
pnpm tauri build
```

`pnpm dev`로 실행하는 브라우저 dev fallback은 `src/lib/app-state.ts`와 `src/lib/tauri-client.ts`에 정의된 mock snapshot으로 React UI를 렌더링합니다. Tauri를 띄우지 않아도 설정 패널·queue·모델 매니저를 dogfooding할 수 있습니다.

빈 mock은 모든 화면을 비워두므로, 브라우저 진입점은 `src/lib/dev-fixtures.ts`의 샘플
데이터를 주입합니다. 일주일치 한국어·영어 캡처, 실제 Markdown 본문이 있는 요약과 상세
보고서, 반복 중인 실행 1건, 실패한 실행 1건, 설치된 모델 몇 개가 들어 있습니다.

주입은 opt-in이며 브라우저 전용입니다. `main.tsx`가 Tauri 밖에서 실행될 때만 호출하므로
데스크톱 빌드와 테스트 스위트는 영향을 받지 않습니다. mock은 실제 백엔드 semantics를
그대로 따르므로(`.agent/locked-behaviors.md` §10) 브라우저 dogfooding이 제품 버그를
가리지 않습니다.

## 테스트

- **프론트엔드 (Vitest)** — `pnpm test`
- **백엔드 (cargo)** — `cargo test --manifest-path src-tauri/Cargo.toml`
- **타입** — `pnpm exec tsc --noEmit`

프론트엔드 스위트에는 디자인 시스템 계약 테스트가 있습니다. `src/styles.test.ts`와
`src/components/ui/density-contract.test.tsx`가 스타일시트를 파싱해서, 활성 화면이
하드코딩된 값 대신 공용 spacing·typography·control-height 토큰을 쓰는지, 긴 텍스트가
줄바꿈 가능한지, `prefers-reduced-motion`에서 불필요한 모션이 꺼지는지 검증합니다.
새 크기가 필요하면 토큰을 추가해야 합니다.

## 디렉터리 구조

```
wakenote/
├── index.html                      Vite 진입점
├── package.json                    pnpm + Vite + Vitest + Tauri 스크립트
├── vite.config.ts
├── tsconfig.json
├── PRD.md                          제품 요구사항 (한국어)
├── src/                            React 19 + TS + Tailwind v4 프론트엔드
│   ├── App.tsx
│   ├── main.tsx                    브라우저 진입점 (dev fixtures 주입)
│   ├── styles.css                  tokens/shell/components/pages import
│   ├── styles/                     tokens, shell, components, pages
│   ├── components/
│   │   ├── shell/                  AppFrame, AppSidebar, PageHeader
│   │   ├── capture/                recorder, live transcript, calibration
│   │   ├── meetings/               장시간 회의 화면
│   │   ├── reports/                ReportComposer (새 보고서 대화상자)
│   │   ├── settings/               설정 섹션
│   │   ├── ReportHistoryView.tsx   보고서 목록 + 렌더링된 문서
│   │   └── ui/                     primitives (markdown, empty-state 포함)
│   ├── overlay/                    click-through live caption overlay
│   └── lib/                        tauri-client, app-state, status-summary,
│                                   llm-report-runs, report-composer,
│                                   dev-fixtures,
│                                   onboarding, calibration, types
└── src-tauri/                      Rust 백엔드 (Tauri 2)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── Info.plist
    ├── src/                        audio, capture, commands, live_capture,
    │                               models, persistence, queue, recorder,
    │                               settings, storage, transcription, main
    └── tests/                      각 모듈에 대한 통합 테스트
```

## 알려진 제약

- macOS 전용. M4A 인코딩이 `/usr/bin/afconvert`에 의존하므로 다른 플랫폼에서는 `.wav`만 사용할 수 있고, Tauri auto-launch / 트레이 아이콘 가정도 macOS 기준입니다.
- Whisper 추론은 macOS에서 가능한 경우 `whisper-rs`의 Metal GPU 가속을 사용합니다. 사양이 낮은 Mac에서는 큰 모델이 여전히 느리거나 메모리를 많이 쓸 수 있으므로(예: `whisper-medium`은 상주 메모리 ≈ 1.5 GB로 8 GB MacBook Air에 부담), 즉각적인 피드백이 필요하면 `whisper-small`, 한국어 품질과 속도의 중간값이 필요하면 `whisper-medium`, 정확도가 필요하면 `whisper-turbo` 또는 `whisper-large`를 선택하세요. 선택된 백엔드(Metal/CPU)는 모델 로드 시 `[wakenote] whisper: …` 로그로 남으므로 GPU 사용 여부를 확인할 때 유용합니다.
- `afconvert`는 청크마다 동기적으로 호출되므로, max chunk 값이 매우 길면 그만큼 worker가 더 오래 점유됩니다.
- Warning banner dismiss 상태는 세션 내에서만 유지되며, 앱을 종료하면 잊혀집니다.

## 라이선스

아직 명시되지 않았습니다. 제품 스펙과 acceptance criteria는 `PRD.md`를 참고하세요.
