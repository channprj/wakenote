# WakeNote

[한국어 README](./README.ko.md)

**Voice-activated local transcription for macOS.**

WakeNote is a menu-bar app that listens to one required Primary microphone and an optional Secondary microphone, automatically opens recording chunks when the input crosses a configurable dBFS threshold, and transcribes each chunk locally with Whisper. When both microphones are configured, WakeNote keeps their physical capture streams independent but merges them into one time-aligned recording and transcription input by default. Audio, transcript, and metadata are written next to each other under a date-bucketed folder so recordings stay greppable from notes, scripts, or backup tools.

The app is built on Tauri 2 (Rust backend) + React 19 + TypeScript + Tailwind CSS v4. Transcription runs offline through `whisper-rs` (whisper.cpp) with Metal GPU acceleration on macOS builds; audio capture goes through `cpal`. The default theme color is black `#000`.

## Highlights

- **Voice-activated capture** — recording starts only after RMS dBFS stays above the threshold for the configured *attack* duration, and ends only after it stays below for the *release* duration. Pre-roll and post-roll buffers preserve the head and tail of each utterance.
- **Resilient dual microphones** — select one Primary and an optional Secondary physical input in Settings. Each microphone owns its stream, frame queue, level, warning, and same-device reconnect loop, so one failure never stops the other. With two inputs, **Merge microphone inputs** is on by default and produces one recording and one transcription; turn it off to preserve separate per-microphone recordings and transcript identities. Input monitoring uses Primary only.
- **Independent Recording / Transcription / Pause toggles** — capture audio without transcribing, transcribe an existing backlog without recording, or pause everything from the tray.
- **Local-first storage** — `{save_root}/YYYYMMDD/HHMMSS.{m4a|wav}` for audio, `.txt` for transcripts, `.json` for metadata, `.error.txt` for recoverable transcription errors. Filename collisions roll over to `-001`, `-002`, …
- **Model manager** — download, verify (SHA-256), cancel, delete, and switch models from the UI. Default Korean-capable Whisper registry ships `whisper-small`, `whisper-medium`, `whisper-turbo`, and `whisper-large`; Parakeet V3 and SenseVoice download and run fully on-device via a bundled sherpa-onnx engine (no external tools), and Nemotron 3.5 ASR runs through an external-command adapter.
- **AI summaries and detailed reports** — turn any set of captures into a Markdown document through OpenRouter. The runner drafts, grades its own output against success criteria, and refines until the criteria are met or the iteration budget runs out. See [AI summaries and reports](#ai-summaries-and-reports).
- **Single-flight transcription queue** — at most one job runs at a time; failed jobs surface as recoverable errors with retry / skip actions; recovered jobs from a previous session are re-queued on startup.
- **Robust live capture** — the audio callback dispatches frames to a bounded background queue; if processing falls behind, stale frames are dropped and the UI surfaces a runtime warning instead of stalling the input thread.
- **macOS tray + caption overlay** — tray icon reflects state (Idle / Listening / Recording / Transcribing / Paused / Error) with quick toggles and a `Reveal Save Folder` action. The floating overlay is a click-through caption surface for the active desktop: it shows only the current live/final transcript text and never opens the main WakeNote window.
- **First-run onboarding** — microphone, save folder, model, and calibration steps must each be confirmed before they count as complete; completed calibration automatically applies the suggested live threshold.

## How it works

```
  ┌──────────────────────────────────────────────────────────────────┐
  │ macOS mics ─► independent cpal streams ─► MicrophoneMixer ─────► │
  │                                   (two inputs, default on)       │
  │                                                   SpeechGate ─► │
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
- `src-tauri/src/multi_capture.rs` — coordinates fixed Primary / Secondary runtimes without sharing streams or dispatch queues, then time-aligns and resamples their frames into one input when merging is enabled.
- `src-tauri/src/recorder.rs` — writes `.wav` via `hound`, `.m4a` via macOS `afconvert` (PCM → WAV → AAC/M4A), `.json` metadata, transcript / error sidecars.
- `src-tauri/src/queue.rs` — `TranscriptionQueue`, idempotent enqueue, single-flight `start_next`, retry/skip/cancel.
- `src-tauri/src/transcription.rs` — `WhisperTranscriber` + `TranscriptionWorker`.
- `src-tauri/src/models.rs` — model registry, download with progress/cancel/checksum, on-disk `ModelStore`.
- `src-tauri/src/persistence.rs` — atomic JSON writes for `settings.json` and `transcription-queue.json` under the app data dir; in-flight jobs recovered as pending on startup.
- `src/App.tsx`, `src/components/*` — settings UI, onboarding strip, level meter, queue panel, model manager, tray preview.
- `src/overlay/*` — separate Tauri overlay entrypoint for the click-through live caption window.

## Output layout

With two microphones enabled, the default merged mode writes one file such as
`142301-mic-merged.m4a` and one transcription job. Its JSON metadata includes
both configured devices in `microphone_inputs`. Turning **Merge microphone
inputs** off writes separate files such as `142301-mic-primary-wired.m4a` and
`142301-mic-secondary-wireless.m4a`; those sidecars include `device_id`,
`device_name`, and `microphone_slot`, and Transcripts interleaves both sources
chronologically with device-name badges and filters. `System Default` is
supported for a single-microphone setup only; a two-microphone setup requires
two explicit, distinct physical devices.

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
| Threshold | `-40 dBFS` | `-90 … -10` |
| Mic input volume | `100%` | `0 … 200` |
| Attack | `200 ms` | `50 … 2 000` |
| Release | `1 000 ms` | `250 … 5 000` |
| Pre-roll | `400 ms` | `0 … 1 500` |
| Lead-in | `200 ms` | `0 … 2 000` |
| Post-roll | `400 ms` | `0 … 2 000` |
| Min chunk | `800 ms` | `100 … 5 000` |
| Max chunk | `180 000 ms` (3 min) | `10 000 … 900 000` |
| Audio format | `m4a` | `m4a` / `wav` |
| Merge microphone inputs | `on` | `on` / `off` |
| Save root | `~/Documents/WakeNote` | any directory |
| Default model | `whisper-medium` | from registry |
| Model directory | `~/Library/Application Support/WakeNote/models` | any directory |
| Transcription language | `ko` | `auto`, `ko`, `en`, `ja`, `zh`, `es`, `fr`, `de` |
| Hide low-confidence transcripts | `on` | `on` / `off` |
| Start input on launch | `on` | `on` / `off` |

Settings are persisted to `<app_data_dir>/settings.json` and clamped to safe ranges on every patch.

## AI summaries and reports

Transcription is only half the point: the reason to record a meeting is to read
what it amounted to afterwards. WakeNote turns captures into a Markdown document
through OpenRouter.

### The workspace

| Screen | What it is for |
| --- | --- |
| **Capture** | Live recording, levels, calibration, and the newest decoded phrases. |
| **Transcripts** | The per-day archive of short captures. Select some and report on them directly. |
| **Meetings** | Long-form imported recordings, transcribed segment by segment. |
| **Reports** | Where reports are created and read. |
| **Activity** | The transcription queue: pending, running, completed, failed. |

### Two kinds of report

| Kind | Shape | Cost |
| --- | --- | --- |
| **Summary** | One-line summary, key points, decisions, action items, open questions. | One draft plus one grading pass. |
| **Detailed report** | Adds context, chronological detail, risks, and evidence notes tied back to specific captures. | Up to `llm_max_iterations` draft/grade rounds, so roughly 3× a summary by default. |

Both prompts are editable templates in **Settings › Integrations**
(`llm_summary_prompt_template`, `llm_report_prompt_template`) with
`{{transcripts}}`, `{{date_range}}`, and `{{selected_count}}` placeholders. Both
instruct the model to use only the supplied transcript as evidence and to write
in the transcript's dominant language.

### Generating one

Two entry points, one dialog:

- **Reports › New report** — choose the kind, then tick which capture days to
  cover. The dialog states the scope, the model, and the iteration budget before
  you spend anything.
- **Transcripts › select captures → Summary / Report** — a shortcut when you are
  already looking at the captures you want covered.

Generation needs an OpenRouter API key (**Settings › Integrations**). One report
runs at a time; a run in flight can be stopped, and a cancelled or failed run can
be retried. Progress is reported per stage — preparing, generating, evaluating,
refining, saving — with the grader's feedback visible as it goes.

### Reading one

Reports render as documents, not raw Markdown: heading hierarchy, GFM tables,
task lists, fenced code, and blockquotes. Each report shows the model used,
iterations spent, captures covered, date range, token counts, and OpenRouter cost,
plus the grader's closing quality feedback. From there you can **Copy** the
Markdown, **Download Markdown** to a file, or **Run again** to regenerate.

Report bodies are written to `{save_root}/reports/` and stay on disk. Hiding a
report removes it from the list only — nothing is deleted.

## Additional ASR providers

WakeNote includes registry entries for `parakeet-tdt-0.6b-v3`, `sensevoice-small`, and `nemotron-3.5-asr-streaming-0.6b`.

### Parakeet V3 and SenseVoice (on-device, no external tools)

Both run **in-process via a bundled [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (onnxruntime) engine** — like the bundled whisper.cpp engine, there is no external CLI or Python to install. Click **Download** in the model list: WakeNote fetches the official ONNX archive from the sherpa-onnx releases, extracts it under the model directory, and the model is then Ready to select. `Delete` removes the extracted model.

The engine is built behind the `asr-sherpa` Cargo feature. The shipped app (`pnpm build`) enables it automatically; a plain `cargo build`/`cargo test` stays light and omits it (selecting one of these models without it returns an "asr-sherpa feature" error). The prebuilt onnxruntime + sherpa-onnx dylibs are downloaded at build time and bundled into the app's `Contents/Frameworks` (with an `@executable_path/../Frameworks` rpath) so the shipped `.app` is self-contained — switch sherpa-rs to its `static` feature to link statically instead.

### Nemotron 3.5 ASR (external command)

NVIDIA ships Nemotron 3.5 ASR as a NeMo checkpoint with no ONNX export, so it can't run on the bundled sherpa-onnx engine. Instead it runs through the external-command adapter: install your preferred NeMo runner, then connect it with a `<model_directory>/nemotron-3.5-asr-streaming-0.6b.command` file. WakeNote marks the model Ready once that file exists; the command reads `WAKENOTE_AUDIO_PATH`, `WAKENOTE_MODEL_ID`, `WAKENOTE_MODEL_DIRECTORY`, and `WAKENOTE_LANGUAGE`, then writes the transcript to stdout.

### Custom external-command models (advanced)

For any other engine, place a shell command file at `<model_directory>/<model-id>.command`; WakeNote marks that model ready when the file exists. The command reads `WAKENOTE_AUDIO_PATH`, `WAKENOTE_MODEL_ID`, `WAKENOTE_MODEL_DIRECTORY`, and `WAKENOTE_LANGUAGE`, then writes the transcript to stdout.

## Requirements

- macOS 11.0+ (Big Sur or newer) — the shipped app is an arm64 build and bundles onnxruntime, both of which require 11.0.
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

# 4. Build the frontend bundle (used by Tauri's beforeBuildCommand)
pnpm build

# 5. Build the macOS .app and install it to /Applications
pnpm build install            # release build + install
pnpm build install open       # release build + install + launch
pnpm build debug install      # debug build + install
pnpm build install --path ~/Applications

# Convenience aliases (same as above):
#   pnpm dmg
#   pnpm dmg:debug
#   pnpm build:install
#   pnpm build:install:open
#   pnpm build:install:debug
#   pnpm build:install:debug:open
```

`pnpm build` with no arguments runs the frontend build (`tsc && vite build`) so
Tauri's `beforeBuildCommand` keeps working. Adding positional arguments invokes
`scripts/build.mjs`, which orchestrates the Tauri bundle and the macOS install.
Override the install directory with `WAKENOTE_INSTALL_PATH=…` or `--path …`.

The browser dev fallback (`pnpm dev`) renders the React UI against a mock snapshot defined in `src/lib/app-state.ts` and `src/lib/tauri-client.ts`, so the settings panel, queue, and model manager are dogfoodable without launching Tauri.

## Browser dev fallback

`pnpm dev` serves the frontend on its own with a mock backend, which is the
fastest way to work on UI without a Tauri rebuild. Because an empty mock leaves
every screen blank, the browser entrypoint seeds it with sample content from
`src/lib/dev-fixtures.ts`: a week of Korean and English captures, a finished
summary and detailed report with real Markdown bodies, one run in flight
mid-iteration, one failed run, and a few installed models.

Seeding is opt-in and browser-only — `main.tsx` calls it just when the app is
running outside Tauri, so the desktop build and the test suite are untouched. The
mock also mirrors real backend semantics (see `.agent/locked-behaviors.md` §10)
so dogfooding in a browser does not hide product bugs.

## Testing

- **Frontend (Vitest)** — `pnpm test`
- **Backend (cargo)** — `cargo test --manifest-path src-tauri/Cargo.toml`
- **Types** — `pnpm exec tsc --noEmit`

The frontend suite includes design-system contract tests: `src/styles.test.ts`
and `src/components/ui/density-contract.test.tsx` parse the stylesheets and
assert that active screens use the shared spacing, typography, and control-height
tokens rather than hardcoded values, that long text stays wrappable, and that
nonessential motion is disabled under `prefers-reduced-motion`. Adding a new
size means adding a token.

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
│   ├── main.tsx                    browser entry (seeds dev fixtures)
│   ├── styles.css                  imports tokens/shell/components/pages
│   ├── styles/                     tokens, shell, components, pages
│   ├── components/
│   │   ├── shell/                  AppFrame, AppSidebar, PageHeader
│   │   ├── capture/                recorder, live transcript, calibration
│   │   ├── transcripts/            player dock
│   │   ├── meetings/               long-form meeting views
│   │   ├── reports/                ReportComposer (new-report dialog)
│   │   ├── settings/               settings sections
│   │   ├── ReportHistoryView.tsx   report list + rendered document
│   │   ├── QueuePanel.tsx          transcription queue
│   │   └── ui/                     primitives incl. markdown, empty-state
│   ├── overlay/                    click-through live caption overlay
│   └── lib/                        tauri-client, app-state, status-summary,
│                                   llm-report-runs, report-composer,
│                                   dev-fixtures, onboarding, calibration, types
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
- Whisper inference uses `whisper-rs` with Metal GPU acceleration on macOS when available. Large models can still be slower or memory-heavy on lower-end Macs (e.g. `whisper-medium` ≈ 1.5 GB resident, which pressures an 8 GB MacBook Air); pick `whisper-small` for fast feedback, `whisper-medium` for a Korean-capable middle ground, or `whisper-turbo`/`whisper-large` for accuracy. The selected backend (Metal vs CPU) is logged at model load as `[wakenote] whisper: …` — useful when confirming GPU use.
- `afconvert` is invoked synchronously per chunk; very long max-chunk values will block the worker for longer.
- Warning-banner dismissal is session-local; closing the app forgets the dismissed state.

## License

Not yet specified. See `PRD.md` for the product spec and acceptance criteria.
