# WakeNote Architecture

## Overview

WakeNote is a macOS Tauri 2 application with two user interfaces and one Rust backend:

- the main React workspace for capture, history, reports, queue activity, and settings;
- a separate click-through React overlay for live captions and dictation feedback;
- the Rust process that owns audio devices, recording, transcription runtimes, persistence, permissions, tray behavior, and Tauri commands.

The central architectural boundary is the filesystem. A capture becomes useful only after its audio and metadata are committed. Transcription then adds a success or error sidecar, while the persistent queue records processing state. This keeps recordings recoverable even if transcription, a cloud provider, or the UI fails.

## Components

### Frontend workspace

`src/main.tsx` mounts `src/App.tsx`. Outside Tauri it also installs the browser-only fixtures from `src/lib/dev-fixtures.ts`; inside Tauri all state comes through `src/lib/tauri-client.ts`.

The primary routes are defined in `src/lib/navigation.ts`:

| Route | Responsibility |
| --- | --- |
| Capture | Live levels, recorder state, calibration, and recent partial/final text |
| Meetings | Imported or manually recorded long-form audio and resumable transcription |
| Transcripts | Date-based capture archive, playback, selection, and report entry points |
| Reports | Report composition, run progress, history, and rendered Markdown |
| Activity | Persistent transcription queue, warnings/errors, retry, reprocessing, and recovery actions |
| Settings | General, Audio, Dictation, Models, Storage, Integrations, and Advanced configuration |

`src/lib/app-state.ts` reduces snapshots and events into frontend state. Domain helpers in `src/lib/` keep capability filtering, queue presentation, meeting progress, report composition, and browser mocks out of page components.

### Caption overlay

`src/overlay/main.tsx` mounts `RecordingOverlay.tsx` into the separate Tauri overlay window. The backend emits overlay state and live transcript events; the overlay never owns capture or transcription state. It is a click-through presentation surface, while the main window remains the control surface.

### Tauri command and event bridge

`src-tauri/src/main.rs` builds the Tauri application, manages shared runtime state, registers commands, starts background workers, and emits events. `src/lib/tauri-client.ts` is the frontend adapter for those commands and provides a compatible in-memory backend for browser development.

Commands cover settings, credentials, permissions, models, queue actions, capture, transcript indexes, reports, costs, source capture, and meetings. Events carry high-frequency or asynchronous updates such as live transcript partials, report runs, meeting progress, source detection, audio merge progress, and cost snapshots.

### Audio capture and recording

| Module | Responsibility |
| --- | --- |
| `audio.rs` | Speech gate, level monitoring, calibration inputs, and capture timing |
| `live_capture.rs` | CPAL input runtime and bounded frame dispatch |
| `multi_capture.rs` | Independent Primary/Secondary streams, recovery, resampling, and time-aligned merge |
| `system_audio.rs` | ScreenCaptureKit-backed system-audio capture |
| `source_watcher.rs` / `sources.rs` | Recognized application/window sources and lifecycle detection |
| `capture.rs` | Converts gated frames into completed chunks |
| `recorder.rs` | Writes WAV directly, M4A through `afconvert`, MP3 through `ffmpeg`, plus metadata and sidecars |
| `storage.rs` | Allocates dated, collision-safe output paths and uploaded-audio paths |
| `audio_analysis.rs` / `audio_merge.rs` | Waveform analysis and non-destructive audio merge operations |

Live callbacks push frames into bounded queues so filesystem or inference work does not block the audio callback. Each selected microphone has its own stream and health state. When merge is enabled, the downstream recorder receives one synchronized input while metadata retains both physical devices.

### Transcription runtimes

`src-tauri/src/models.rs` is the capability registry and local model store. A descriptor declares whether a model supports file transcription, realtime use, streaming, diarization, cost reporting, request-size limits, and each selectable context.

`src-tauri/src/transcription.rs` routes a request by `provider_runtime` and returns a common `TranscriptionExecution` with text, requested/effective model provenance, optional fallback provenance, provider usage, and typed failures. The available runtime families are:

- local `whisper-rs` with Metal on macOS;
- bundled sherpa-onnx models;
- isolated Qwen3-ASR Python environments;
- advanced external commands;
- OpenRouter file transcription;
- OpenAI file, realtime, and diarization endpoints;
- Soniox async REST and realtime WebSocket endpoints.

`cloud_transcription.rs` owns shared cloud credentials and failure categories. `openai_realtime.rs` and `soniox_realtime.rs` keep provider protocols separate while sharing the provider-neutral request, partial, and final-result store in `cloud_realtime.rs`. `soniox_async.rs` owns Soniox upload, polling, result retrieval, and best-effort remote cleanup.

### Queue and Activity

`queue.rs` defines the durable queue, job lifecycle, issue severity, acknowledgement state, and history pruning. `commands.rs` connects captures and imports to the queue and persists every meaningful transition.

The production worker runs at most one transcription job at a time. A job retains its chosen model and transcription options. On startup, a persisted `running` job is rewritten to `pending`, allowing the worker to retry work interrupted by an app exit. Completed no-speech, empty, low-confidence, and artifact outcomes can carry warnings without being represented as processing failures.

Activity acknowledgement is separate from job lifecycle: marking an outcome resolved sets its read state but does not delete the queue record or its audio. Reprocessing creates a new pending attempt with the selected model.

### Dictionary

`dictionary.rs` compiles enabled canonical terms and aliases into provider context plus deterministic post-transcription correction. `dictionary_file.rs` synchronizes the same entries through `dictionary.txt`, preserving the last valid configuration when a file edit is invalid.

Provider-native context is used only when supported. Deterministic correction remains the common final step, so local and cloud results follow the same canonical-term contract.

### Dictation

`dictation.rs` owns shortcut validation, push-to-talk state, dedicated microphone capture, the ten-minute guard, and its temporary inference audio. `text_input.rs` inserts a non-empty final transcript into the focused macOS application and restores the previous clipboard contents.

Dictation uses the configured Primary microphone but does not interrupt the archival voice-activated recorder. Its audio, metadata, transcript, or recoverable error sidecar is stored like any other capture; only temporary inference files are discarded.

### Meetings

`meeting.rs` keeps long-form work separate from short capture chunks. Imported `mp3`, `m4a`, or `wav` files are copied into `meetings/<id>/`, normalized, split into bounded segments, and checkpointed to `meeting.json` after each segment. Interrupted work can resume from persisted progress.

OpenAI GPT-4o Transcribe Diarize can persist speaker turns when speaker separation is requested. Other compatible models use the standard segment transcript path.

### Reports

`llm.rs` builds transcript evidence, calls the configured OpenRouter model, evaluates/refines the result, and atomically writes Markdown plus metadata. `llm_runs.rs` persists active and terminal run state under `reports/.runs/`, enabling cancellation, retry, and startup reconciliation.

The frontend renders report Markdown without enabling raw HTML. Hiding a report changes list visibility only; it does not remove the report files.

## Data flow

### Voice-activated capture and queued transcription

```text
physical microphones / supported system source
  -> independent capture runtimes and bounded frame queues
  -> level monitor + SpeechGate
  -> optional Primary/Secondary time-aligned merge
  -> Recorder
  -> YYYYMMDD audio + metadata sidecar
  -> persistent TranscriptionQueue
  -> capability-selected RuntimeTranscriber
  -> Dictionary correction + quality classification
  -> transcript or recoverable error sidecar
  -> snapshot/events -> Transcripts and Activity
```

The audio and metadata are committed before the queue processes them. The transcript sidecar is therefore additive rather than the only durable representation of a capture.

### Realtime captions

```text
cumulative live samples
  -> selected OpenAI or Soniox realtime manager
  -> send only unseen PCM samples
  -> provider partials -> shared callback -> overlay event
  -> capture commit -> provider final result
  -> path-keyed shared result store
  -> normal transcription worker and artifact pipeline
```

Provider-specific protocol state does not leak into the recorder or frontend. A final result still goes through the common provenance, Dictionary, warning/error, and sidecar contracts.

### Shortcut dictation

```text
global shortcut pressed
  -> dedicated Primary capture + feedback overlay
  -> shortcut released -> saved dictation artifact
  -> selected dictation model
  -> non-empty final text -> transcript sidecar
  -> focused-cursor insertion + clipboard restoration
```

### Meetings

```text
imported or manually recorded audio
  -> meetings/<id>/audio.* + meeting.json
  -> normalization and segmentation
  -> compatible model per segment or provider request
  -> atomic meeting.json progress checkpoints
  -> completed transcript / speaker turns -> Meetings UI
```

### Reports

```text
selected transcript evidence
  -> persisted report run
  -> OpenRouter draft
  -> grading and bounded refinement
  -> reports/*.md + reports/*.json
  -> rendered document and run history
```

## Persistence boundaries

WakeNote deliberately separates private application state from the user-visible archive.

### Private app-data directory

Tauri resolves the platform app-data directory for `com.chann.wakenote`. On macOS this is normally under `~/Library/Application Support/com.chann.wakenote/`.

| File | Contents |
| --- | --- |
| `settings.json` | Non-secret application settings |
| `transcription-queue.json` | Queue jobs, status, issues, selected model/options, and read state |
| `openrouter-secrets.json` | OpenRouter API key only |
| `openai-secrets.json` | OpenAI API key only |
| `soniox-secrets.json` | Soniox API key only |
| `dictionary.txt` | Human-editable canonical terms and aliases |
| `list-visibility.json` | Non-destructive hidden-item state per normalized save root |
| `transcription-costs.json` | Bounded provider usage and local cost estimates |

Secret values are loaded only for backend dispatch. Frontend snapshots expose configured booleans, never the stored keys.

### User-selected save root

The default is `~/Documents/WakeNote`.

```text
<save_root>/
├── YYYYMMDD/
│   ├── YYMMDD-HHMMSS[-source][-N].{m4a|mp3|wav}
│   ├── YYMMDD-HHMMSS[-source][-N].json
│   ├── YYMMDD-HHMMSS[-source][-N].txt
│   ├── YYMMDD-HHMMSS[-source][-N].error.txt
│   └── all.json
├── uploaded/YYYYMMDD/
├── meetings/<meeting-id>/
└── reports/
    ├── *.md
    ├── *.json
    └── .runs/*.json
```

`all.json` is a derived per-day transcript index; audio and individual sidecars remain the source artifacts. Collisions use `-2`, `-3`, and so on rather than overwriting an existing basename.

## Directory structure

```text
wakenote/
├── README.md / README.ko.md    Project front doors
├── USAGE.md                    Operational and development reference
├── ARCHITECTURE.md             This document
├── PRD.md                      Product requirements
├── package.json                pnpm scripts and frontend dependencies
├── scripts/                    Build, install, and DMG orchestration
├── src/
│   ├── App.tsx                 Main workspace composition
│   ├── main.tsx                Main browser/Tauri frontend entry
│   ├── components/             Product pages, settings, shell, and UI primitives
│   ├── lib/                    State, Tauri adapter, fixtures, and domain helpers
│   ├── overlay/                Separate caption/dictation window
│   └── styles/                 Shared tokens and page/component styles
└── src-tauri/
    ├── Cargo.toml              Rust dependencies and optional asr-sherpa feature
    ├── tauri.conf.json         Window, bundle, resources, and macOS minimum version
    ├── src/                    Backend domains and Tauri runtime
    ├── tests/                  Rust integration tests
    └── vendor/                 Patched sherpa-rs / sherpa-rs-sys sources
```

## Design decisions

### Files before inference

WakeNote commits recoverable audio and metadata before transcription. Queue or provider failure cannot erase the original recording, and reprocessing can reuse the same artifact.

### Capabilities, not provider-name conditionals in the UI

Model descriptors define selectable contexts, streaming, diarization, and fallback compatibility. The frontend filters by those capabilities, and the backend normalizes unsupported options again before dispatch.

### Explicit network use

Local models are the default. Cloud runtimes require a saved provider key and retain the selected model on failure. WakeNote does not silently switch between cloud providers. A user-enabled cost-limit fallback is narrower: it retries once with a selected ready on-device model only for a classified billing-limit failure.

### Provider isolation with shared result contracts

OpenAI and Soniox protocols have separate adapters and error parsing. They converge only at credential redaction, typed failures, realtime partial/final contracts, usage accounting, Dictionary correction, and artifact persistence.

### Non-destructive history controls

Resolved Activity outcomes and hidden transcripts, meetings, or reports remain stored. Destructive actions are explicit and separate from acknowledgement or visibility state.

### Atomic small-state writes

Settings, queue state, meeting progress, report metadata, Dictionary synchronization, and visibility registries use temporary-file replacement patterns. Startup repair is bounded to known recoverable states such as `running` queue jobs and interrupted report/meeting work.

### Browser development is a UI simulator

The browser fallback intentionally uses fixtures and a mock Tauri adapter. It is useful for visual and interaction development but cannot prove microphone capture, filesystem behavior, permissions, native focused-cursor insertion, or provider networking.
