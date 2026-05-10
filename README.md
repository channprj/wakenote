# WakeNote

[한국어 README](./README.ko.md)

**Voice-activated local transcription for macOS.**

WakeNote is a menu-bar app that listens to a chosen microphone, automatically opens a recording chunk when the input crosses a configurable dBFS threshold, and transcribes each chunk locally with Whisper. Audio, transcript, and metadata are written next to each other under a date-bucketed folder so recordings stay greppable from notes, scripts, or backup tools.

The app is built on Tauri 2 (Rust backend) + React 19 + TypeScript + Tailwind CSS v4. Transcription runs offline through `whisper-rs` (whisper.cpp); audio capture goes through `cpal`. The default theme color is black `#000`.

## Highlights

- **Voice-activated capture** — recording starts only after RMS dBFS stays above the threshold for the configured *attack* duration, and ends only after it stays below for the *release* duration. Pre-roll and post-roll buffers preserve the head and tail of each utterance.
- **Independent Recording / Transcription / Pause toggles** — capture audio without transcribing, transcribe an existing backlog without recording, or pause everything from the tray.
- **Local-first storage** — `{save_root}/YYYYMMDD/HHMMSS.{m4a|wav}` for audio, `.txt` for transcripts, `.json` for metadata, `.error.txt` for recoverable transcription errors. Filename collisions roll over to `-001`, `-002`, …
- **Whisper model manager** — download, verify (SHA-256), cancel, delete, and switch models from the UI. Default registry ships `whisper-medium` and `whisper-tiny` from `ggerganov/whisper.cpp`.
- **Single-flight transcription queue** — at most one job runs at a time; failed jobs surface as recoverable errors with retry / skip actions; recovered jobs from a previous session are re-queued on startup.
- **Robust live capture** — the audio callback dispatches frames to a bounded background queue; if processing falls behind, stale frames are dropped and the UI surfaces a runtime warning instead of stalling the input thread.
- **macOS tray + floating overlay** — tray icon reflects state (Idle / Listening / Recording / Transcribing / Paused / Error) with quick toggles and a `Reveal Save Folder` action. The floating overlay only appears when recording or transcribing.
- **First-run onboarding** — microphone, save folder, model, and calibration steps must each be confirmed before they count as complete; completed calibration automatically applies the suggested live threshold.

## How it works

```
  ┌──────────────────────────────────────────────────────────────────┐
  │ macOS mic ─► cpal stream ─► bounded frame queue ─► SpeechGate ─► │
  │                                                          │       │
  │                              ┌───────────────────────────┘       │
  │                              ▼                                   │
  │           pre-roll buffer ─► CaptureController ─► .m4a / .wav    │
  │                                          │       .json metadata  │
  │                                          ▼                       │
  │                       single-flight TranscriptionQueue ──► .txt  │
  │                                          │              .error.txt│
  │                                          ▼                       │
  │                       whisper-rs (local Whisper inference)       │
  └──────────────────────────────────────────────────────────────────┘
```

- `src-tauri/src/audio.rs` — `SpeechGate` (attack/release/pre-roll/post-roll/min/max chunk), `LevelMonitor` (current/peak dBFS, noise floor, suggested threshold).
- `src-tauri/src/capture.rs` — `CaptureController` / `CaptureProcessor` turn raw frames into completed `RecordedChunk`s.
- `src-tauri/src/live_capture.rs` — `LiveCaptureRuntime` + `CpalAudioInput`; bounded `FrameDispatcher` drops stale frames under back-pressure.
- `src-tauri/src/recorder.rs` — writes `.wav` via `hound`, `.m4a` via macOS `afconvert` (PCM → WAV → AAC/M4A), `.json` metadata, transcript / error sidecars.
- `src-tauri/src/queue.rs` — `TranscriptionQueue`, idempotent enqueue, single-flight `start_next`, retry/skip/cancel.
- `src-tauri/src/transcription.rs` — `WhisperTranscriber` + `TranscriptionWorker`.
- `src-tauri/src/models.rs` — model registry, download with progress/cancel/checksum, on-disk `ModelStore`.
- `src-tauri/src/persistence.rs` — atomic JSON writes for `settings.json` and `transcription-queue.json` under the app data dir; in-flight jobs recovered as pending on startup.
- `src/App.tsx`, `src/components/*` — settings UI, onboarding strip, level meter, queue panel, model manager, tray preview, floating overlay.

## Output layout

```
~/Documents/WakeNote/
└── 20260509/
    ├── 142301.m4a          # audio (or .wav)
    ├── 142301.json         # ChunkMetadata: model, device, sample rate,
    │                       # threshold, started_at, ended_at, duration_ms,
    │                       # transcription_status, app_version, …
    ├── 142301.txt          # successful transcript
    └── 142301.error.txt    # recoverable transcription error (if any)
```

## Default capture settings

| Setting | Default | Range |
| --- | --- | --- |
| Threshold | `-60 dBFS` | `-90 … -10` |
| Attack | `300 ms` | `50 … 2 000` |
| Release | `1 500 ms` | `250 … 5 000` |
| Pre-roll | `300 ms` | `0 … 1 500` |
| Post-roll | `300 ms` | `0 … 2 000` |
| Min chunk | `500 ms` | `100 … 5 000` |
| Max chunk | `120 000 ms` (2 min) | `10 000 … 900 000` |
| Audio format | `m4a` | `m4a` / `wav` |
| Save root | `~/Documents/WakeNote` | any directory |
| Default model | `whisper-medium` | from registry |
| Model directory | `~/Library/Application Support/WakeNote/models` | any directory |
| Start input on launch | `on` | `on` / `off` |

Settings are persisted to `<app_data_dir>/settings.json` and clamped to safe ranges on every patch.

## Requirements

- macOS 10.15+ (Catalina or newer).
- Node.js 20+ and `pnpm` 10 (the repo pins `pnpm@10.33.4` via `packageManager`).
- Rust toolchain (stable, `edition = "2024"`).
- Xcode Command Line Tools — needed for `afconvert` (M4A encoding) and the Tauri build chain.
- Microphone permission for the WakeNote app on first launch.

## Getting started

```bash
# 1. Install JS dependencies
pnpm install

# 2. Run the desktop app in dev mode (Vite + Tauri)
pnpm tauri dev

# 3. Run only the frontend (browser dev fallback with mock backend)
pnpm dev

# 4. Build a release bundle
pnpm tauri build
```

The browser dev fallback (`pnpm dev`) renders the React UI against a mock snapshot defined in `src/lib/app-state.ts` and `src/lib/tauri-client.ts`, so the settings panel, queue, and model manager are dogfoodable without launching Tauri.

## Testing

- **Frontend (Vitest)** — `pnpm test`
- **Backend (cargo)** — `cargo test --manifest-path src-tauri/Cargo.toml`

## Project layout

```
wakenote/
├── index.html                      Vite entry
├── package.json                    pnpm + Vite + Vitest + Tauri scripts
├── vite.config.ts
├── tsconfig.json
├── PRD.md                          Product requirements (KO)
├── src/                            React 19 + TS + Tailwind v4 frontend
│   ├── App.tsx
│   ├── styles.css
│   ├── components/                 SettingsPanel, ModelManager, QueuePanel,
│   │                               LevelMeter, TrayPreview, Onboarding,
│   │                               FloatingOverlay, ui/primitives
│   └── lib/                        tauri-client, app-state, status-summary,
│                                   onboarding, calibration, types
└── src-tauri/                      Rust backend (Tauri 2)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── Info.plist
    ├── src/                        audio, capture, commands, live_capture,
    │                               models, persistence, queue, recorder,
    │                               settings, storage, transcription, main
    └── tests/                      integration tests for each module
```

## Known limitations

- macOS only. M4A encoding shells out to `/usr/bin/afconvert`; on other platforms only `.wav` would be available, and Tauri auto-launch / tray-icon assumptions are macOS-flavored.
- Whisper inference is CPU-bound through `whisper-rs`. Large models will be slower than real time on lower-end Macs; pick `whisper-tiny` for fast feedback or `whisper-medium` for accuracy.
- `afconvert` is invoked synchronously per chunk; very long max-chunk values will block the worker for longer.
- Warning-banner dismissal is session-local; closing the app forgets the dismissed state.

## License

Not yet specified. See `PRD.md` for the product spec and acceptance criteria.
