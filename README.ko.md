# WakeNote

[English README](./README.md)

**macOS 전용 음성 활성화 로컬 transcription 앱.**

WakeNote은 메뉴바 앱입니다. 선택한 마이크 입력을 모니터링하다가 dBFS 임계값을 넘으면 자동으로 녹음 청크를 시작하고, 청크가 끝나면 로컬 Whisper로 transcription을 수행합니다. 오디오, 텍스트, 메타데이터는 모두 같은 폴더(날짜별 디렉터리) 안에 나란히 저장되어 노트·스크립트·백업 도구로 바로 grep할 수 있습니다.

기술 스택은 Tauri 2 (Rust 백엔드) + React 19 + TypeScript + Tailwind CSS v4입니다. Transcription은 macOS 빌드에서 Metal GPU 가속이 켜진 `whisper-rs` (whisper.cpp)로 오프라인 추론하며, 오디오 캡처는 `cpal`을 사용합니다. 기본 테마 색상은 블랙 `#000`입니다.

## 핵심 특징

- **음성 활성화 캡처** — RMS dBFS가 임계값 위로 *attack* 시간 이상 유지되어야 녹음이 시작되고, 임계값 아래로 *release* 시간 이상 유지되어야 종료됩니다. pre-roll / post-roll 버퍼로 발화의 시작과 끝이 잘리지 않게 보존합니다.
- **녹음 / transcription / 일시정지 토글 분리** — 텍스트 없이 오디오만 저장, 신규 녹음 없이 기존 backlog만 transcription, 또는 트레이에서 전체 일시정지 가능.
- **로컬 우선 저장** — `{save_root}/YYYYMMDD/HHMMSS.{m4a|wav}` 오디오, `.txt` 전사, `.json` 메타데이터, 복구 가능한 transcription 오류는 `.error.txt`. 파일명이 충돌하면 `-001`, `-002` 식으로 자동 롤오버.
- **모델 매니저** — UI에서 모델을 다운로드 / 검증(SHA-256) / 설치 / 취소 / 삭제 / 전환할 수 있습니다. 한국어 사용 가능한 기본 Whisper 레지스트리는 `whisper-tiny`, `whisper-small`, `whisper-medium`, `whisper-turbo`, `whisper-large`를 제공합니다. Parakeet V3, Nemotron 3.5 ASR, SenseVoice는 런타임 CLI를 통해 온디바이스로 설치되고, Cohere Transcribe는 API 키로 클라우드에서 동작합니다.
- **단일 실행 transcription queue** — 동시에 한 작업만 실행. 실패한 작업은 복구 가능한 오류로 표시되고 retry / skip 가능. 이전 세션에서 running 상태였던 작업은 시작 시 pending으로 자동 복구됩니다.
- **견고한 라이브 캡처** — 오디오 콜백은 프레임을 bounded 백그라운드 큐에 넘깁니다. 처리가 입력 속도를 못 따라가면 오래된 프레임을 drop하고 입력 스레드를 막지 않으며, UI에는 runtime warning을 띄웁니다.
- **macOS 트레이 + Floating overlay** — 트레이 아이콘이 상태(Idle / Listening / Recording / Transcribing / Paused / Error)를 색으로 보여주며, 빠른 토글과 `Reveal Save Folder` 액션을 제공합니다. Floating overlay는 녹음 또는 transcription 중일 때만 나타납니다.
- **첫 실행 onboarding** — 마이크, 저장 폴더, 모델, 캘리브레이션 단계는 각각 명시적으로 확인되어야 complete 처리됩니다. 캘리브레이션이 끝나면 라이브 신호 기반 suggested threshold가 자동 적용됩니다.

## 동작 흐름

```
  ┌──────────────────────────────────────────────────────────────────┐
  │ macOS mic ─► cpal stream ─► bounded frame queue ─► SpeechGate ─► │
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
- `src-tauri/src/recorder.rs` — `hound`로 `.wav` 작성, macOS `afconvert`로 `.m4a` 작성 (PCM → WAV → AAC/M4A), `.json` 메타데이터 및 transcript / error sidecar.
- `src-tauri/src/queue.rs` — `TranscriptionQueue`, idempotent enqueue, 단일 실행 `start_next`, retry / skip / cancel.
- `src-tauri/src/transcription.rs` — `WhisperTranscriber` + `TranscriptionWorker`.
- `src-tauri/src/models.rs` — 모델 레지스트리, 진행률 / 취소 / 체크섬 검증을 포함한 다운로드, 디스크 `ModelStore`.
- `src-tauri/src/persistence.rs` — app data 디렉터리 아래의 `settings.json`과 `transcription-queue.json`을 atomic하게 저장. 시작 시 in-flight 작업을 pending으로 복구.
- `src/App.tsx`, `src/components/*` — 설정 UI, onboarding strip, level meter, queue panel, 모델 매니저, 트레이 프리뷰, floating overlay.

## 출력 파일 구조

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
| Threshold | `-42 dBFS` | `-90 … -10` |
| Attack | `300 ms` | `50 … 2 000` |
| Release | `600 ms` | `250 … 5 000` |
| Pre-roll | `600 ms` | `0 … 1 500` |
| Post-roll | `300 ms` | `0 … 2 000` |
| Min chunk | `600 ms` | `100 … 5 000` |
| Max chunk | `60 000 ms` (1분) | `10 000 … 900 000` |
| 오디오 포맷 | `m4a` | `m4a` / `wav` |
| 저장 루트 | `~/Documents/WakeNote` | 임의 디렉터리 |
| 기본 모델 | `whisper-medium` | 레지스트리 내 모델 |
| 모델 디렉터리 | `~/Library/Application Support/WakeNote/models` | 임의 디렉터리 |
| Transcription 언어 | `ko` | `auto`, `ko`, `en`, `ja`, `zh`, `es`, `fr`, `de` |
| 저신뢰 transcript 숨기기 | `on` | `on` / `off` |
| 실행 시 입력 자동 시작 | `on` | `on` / `off` |

설정은 `<app_data_dir>/settings.json`에 저장되며, patch가 적용될 때마다 안전 범위로 clamp됩니다.

## 추가 ASR provider

WakeNote 기본 레지스트리에는 `parakeet-tdt-0.6b-v3`, `nemotron-3.5-asr`, `sensevoice-small`, `cohere-transcribe-03-2026` 항목이 포함됩니다.

### 모델 매니저에서 설치하기

오프라인 뉴럴 모델은 모델 목록에서 **Install** 버튼을 누르면 됩니다. WakeNote가 PATH에서 런타임 CLI를 감지하고, 모델 가중치 다운로드를 실행한 뒤 `<model_id>.command` 어댑터를 대신 작성해 줍니다 — 완료되면 모델이 Ready로 표시됩니다. 먼저 런타임 CLI를 한 번 설치하세요:

- **Parakeet V3**, **Nemotron 3.5 ASR**는 FluidAudio CoreML(Apple Neural Engine)로 온디바이스 실행되며 [`macparakeet-cli`](https://github.com/moona3k/macparakeet)가 구동합니다: `brew install moona3k/tap/macparakeet-cli`. Nemotron 3.5는 NeMo 전용(ONNX export 없음)이라 이 CoreML 경로가 오프라인 실행 방법입니다.
- **SenseVoice**는 [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)로 실행됩니다: `pip install sherpa-onnx`. 설치 시 검증된 ONNX tarball을 내려받으며, 생성되는 어댑터는 시작 템플릿이므로 설치된 sherpa-onnx 버전에 맞게 플래그/stdout 파싱을 조정하세요.

### 어댑터 동작 방식

external-command 모델은 `<model_directory>/<model-id>.command`를 실행하며, 이 파일이 존재하면 WakeNote가 모델을 ready로 표시합니다. 따라서 직접 작성하거나 수정해도 됩니다. command는 `WAKENOTE_AUDIO_PATH`, `WAKENOTE_MODEL_ID`, `WAKENOTE_MODEL_DIRECTORY`, `WAKENOTE_LANGUAGE` 환경변수를 읽고 transcript를 stdout으로 출력해야 합니다.

### Cohere Transcribe (클라우드)

Cohere Transcribe는 클라우드 모델이라 다운로드할 것이 없습니다. 설정 → Models의 **Cohere API key** 필드에 키를 입력하면(로컬 저장) 모델이 Ready가 됩니다. 환경변수 `COHERE_API_KEY`/`CO_API_KEY`도 fallback으로 동작합니다. API는 명시적인 language가 필요하며 FLAC, MP3, MPEG, MPGA, OGG, WAV를 받고, 그 밖의 로컬 청크는 업로드 전에 WAV로 변환합니다. Cohere 모델 id로 `.command` 파일을 만들면 API 대신 external command adapter가 실행되므로, 로컬 Cohere runner도 같은 방식으로 붙일 수 있습니다.

## 시스템 요구사항

- macOS 10.15 (Catalina) 이상.
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

## 테스트

- **프론트엔드 (Vitest)** — `pnpm test`
- **백엔드 (cargo)** — `cargo test --manifest-path src-tauri/Cargo.toml`

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
│   ├── styles.css
│   ├── components/                 SettingsPanel, ModelManager, QueuePanel,
│   │                               LevelMeter, TrayPreview, Onboarding,
│   │                               FloatingOverlay, ui/primitives
│   └── lib/                        tauri-client, app-state, status-summary,
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
- Whisper 추론은 macOS에서 가능한 경우 `whisper-rs`의 Metal GPU 가속을 사용합니다. 사양이 낮은 Mac에서는 큰 모델이 여전히 느리거나 메모리를 많이 쓸 수 있으므로, 즉각적인 피드백이 필요하면 `whisper-tiny`, 한국어 품질과 속도의 중간값이 필요하면 `whisper-small`, 정확도가 필요하면 `whisper-medium` 또는 `whisper-large`를 선택하세요.
- `afconvert`는 청크마다 동기적으로 호출되므로, max chunk 값이 매우 길면 그만큼 worker가 더 오래 점유됩니다.
- Warning banner dismiss 상태는 세션 내에서만 유지되며, 앱을 종료하면 잊혀집니다.

## 라이선스

아직 명시되지 않았습니다. 제품 스펙과 acceptance criteria는 `PRD.md`를 참고하세요.
