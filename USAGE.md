# WakeNote Usage

## Installation

### Requirements

- macOS 11.0 or later. The current bundle configuration and native capture paths are macOS-specific.
- Node.js 20+ and `pnpm` 10. The repository pins `pnpm@10.33.4` in `package.json`.
- The stable Rust toolchain; the crate uses Rust edition 2024.
- Xcode Command Line Tools for Tauri builds and the native `/usr/bin/afconvert` M4A encoder.
- CMake for compiling the bundled Whisper runtime (`brew install cmake` on macOS).
- `ffmpeg` when recording MP3 or processing imported media that needs conversion. WakeNote checks common Homebrew and `~/.local/bin` locations before `PATH`.
- Microphone permission for live microphone capture.
- Screen Recording permission when system-audio capture is enabled.
- Accessibility permission when WakeNote must type dictation or automatic transcripts into the focused application.

Qwen3-ASR setup additionally needs `uv` or Python 3.10+ and enough disk space for PyTorch plus the selected model. WakeNote's bundled Whisper and sherpa-onnx paths do not need an external Python environment.

### Install dependencies

```bash
brew install cmake
pnpm install
```

### Run the desktop app in development

```bash
pnpm tauri dev
```

Tauri starts the Vite development server through `beforeDevCommand` and opens the native application. Use this path to test microphones, permissions, filesystem persistence, the tray, the overlay, focused-cursor typing, and provider integrations.

### Run only the frontend

```bash
pnpm dev
```

This serves Vite at `http://127.0.0.1:1420` and installs browser-only fixtures. It is the fastest UI development path, but all native calls use the mock adapter.

### Build and install the macOS app

```bash
# Frontend bundle only
pnpm build

# Release Tauri app
pnpm build release

# Release build, install to /Applications, and optionally open it
pnpm build install
pnpm build install open

# Debug equivalents
pnpm build debug install
pnpm build debug install open
```

The build orchestrator enables the `asr-sherpa` Cargo feature, places its native runtime libraries in `WakeNote.app/Contents/Frameworks`, adds the required runtime search path, and applies an ad-hoc deep signature to the assembled bundle.

Use a different installation directory with either form:

```bash
WAKENOTE_INSTALL_PATH="$HOME/Applications" pnpm build install
pnpm build install --path "$HOME/Applications"
```

### Build a DMG

```bash
# Leave the DMG under src-tauri/target/release/bundle/dmg/
pnpm dmg

# Build and move the newest versioned DMG into ~/Downloads
pnpm dmg:downloads
```

`pnpm dmg:downloads` is a standalone script. Do not pass `dmg:downloads` as an argument to `pnpm build`.

## Quick start

### First launch

1. Open **Settings › Audio** and grant the requested Microphone permission.
2. Select one Primary microphone. Add a distinct Secondary device only if you want two-device capture; `System Default` is limited to a one-microphone configuration.
3. Speak and use **Calibration** to apply a threshold that stays above the room noise floor and below normal speech.
4. Open **Settings › Storage**, choose a save root, and click **Confirm Save Root**.
5. Open **Settings › Models**. Install an on-device model or select a configured cloud model.
6. Return to **Capture**. With Recording and Transcription enabled, crossing the threshold creates a recording and queues it for transcription.

The initial defaults are `~/Documents/WakeNote`, `whisper-medium`, Korean transcription, M4A at 96 kbps, and automatic live input on launch.

### Grant app-list permissions

Accessibility and Screen & System Audio Recording use an app-list workflow. In **Settings › Audio**, press **Open System Settings** for either permission. WakeNote opens the matching Privacy & Security pane and places a small native shelf beside the System Settings window. Drag the WakeNote icon from the shelf into the open list, then enable its switch. macOS remains the authority that adds and grants the permission; WakeNote does not edit the TCC database.

The shelf follows System Settings and closes when the permission is granted, System Settings closes, or you close the helper. Microphone permission continues to use the standard macOS request prompt. The drag shelf is available only when WakeNote is running from an installed `.app` bundle; a direct development executable still opens the correct settings pane but cannot provide an application drag item.

### Workspace

| Screen | Use it for |
| --- | --- |
| Capture | Live microphone state, recording waveform, calibration, and newest decoded phrases |
| Meetings | Manual long-form recording, audio import, resumable transcription, and optional speaker separation |
| Transcripts | Per-day capture history, newest/oldest ordering, 50-item pages, playback, selection, bundle Trash, regeneration, and report creation |
| Reports | Summary or detailed-report composition, progress, history, and rendered Markdown |
| Activity | Per-day queue status, an all-dates warning/error review, newest/oldest ordering, retry, skip, safe running-job cancellation, bulk reprocessing, and Trash actions |
| Settings | General behavior, Audio, Dictation, Models, Storage, Integrations, and Advanced sources |

### Capture lifecycle

The speech gate starts a chunk only after the input remains above Threshold for the Attack duration. It closes only after the input remains below Threshold for Release, while Pre-roll, Lead-in, and Post-roll protect the edges. Max Chunk splits continuous audio into bounded recordings.

With two configured microphones, **Microphone processing** offers three modes. **Separate recordings** writes independent Primary and Secondary artifacts. **Merge Audio** is the default: it buffers up to 220 ms, estimates fractional delay and clock drift, corrects gain and polarity, and blends only while alignment confidence is sufficient. **Priority Audio** instead writes one sufficiently loud, clean source at a time, using switching hysteresis and a 50 ms equal-power crossfade so two room responses are never summed. Every mode retains physical-device identity in metadata and transcript filters.

Recording, Transcription, and Pause are independent controls. You can retain audio without creating new transcription jobs, process an existing backlog without recording, or pause both.

### Output layout

The default save root is `~/Documents/WakeNote`.

```text
~/Documents/WakeNote/
├── 20260805/
│   ├── 260805-091530-mic-merged.m4a
│   ├── 260805-091530-mic-merged.json
│   ├── 260805-091530-mic-merged.txt
│   ├── 260805-092201.json
│   ├── 260805-092201.mp3
│   ├── 260805-092201.error.txt
│   └── all.json
├── uploaded/20260805/
├── meetings/<meeting-id>/
│   ├── audio.m4a
│   └── meeting.json
└── reports/
```

| Artifact | Meaning |
| --- | --- |
| `*.m4a`, `*.mp3`, `*.wav` | Captured or copied audio |
| `*.json` beside a chunk | Model, device, timing, source, status, and provenance metadata |
| `*.txt` | Successful transcript |
| `*.error.txt` | Recoverable transcription error |
| `YYYYMMDD/all.json` | Derived transcript-day index; individual sidecars remain authoritative |
| `uploaded/YYYYMMDD/` | Audio imported into the normal transcription queue |
| `meetings/<id>/audio.m4a` | Durable audio created by manual Meeting recording; imported meeting audio keeps its original supported codec |
| `meetings/<id>/meeting.json` | Long-form meeting state, segments, progress, and optional speaker turns |
| `reports/*.md` and `*.json` | Persisted report body and metadata |
| `reports/.runs/*.json` | Active and terminal report-run state |

WakeNote never overwrites an existing basename. A collision adds `-2`, then `-3`, and so on.

## Command reference

### Project commands

| Command | Purpose |
| --- | --- |
| `pnpm install` | Install locked JavaScript dependencies |
| `pnpm dev` | Run browser-only Vite UI with fixtures |
| `pnpm tauri dev` | Run the native app in development |
| `pnpm build` | Run TypeScript compilation and build the frontend bundle |
| `pnpm build release` | Build the release macOS app bundle |
| `pnpm build debug` | Build the debug macOS app bundle |
| `pnpm build install` | Build and install the release app |
| `pnpm build install open` | Build, install, and launch the release app |
| `pnpm dmg` | Build a release DMG |
| `pnpm dmg:debug` | Build a debug DMG |
| `pnpm dmg:downloads` | Build a release DMG and move it to `~/Downloads` |
| `pnpm preview` | Preview the already-built frontend bundle |
| `pnpm test` | Run the complete frontend Vitest suite once |
| `pnpm exec tsc --noEmit` | Type-check without emitting files |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Run the default-feature Rust test suite |
| `cargo test --manifest-path src-tauri/Cargo.toml --all-features` | Run Rust tests including bundled sherpa-onnx integration |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | Check Rust formatting |

### Build options

`scripts/build.mjs` accepts these positional arguments and flags through `pnpm build`:

| Argument | Effect |
| --- | --- |
| `debug`, `--debug` | Use the Tauri debug profile |
| `release`, `--release` | Use the release profile; this is the default when packaging |
| `app` | Package the `.app` bundle |
| `dmg` | Package a DMG |
| `install` | Copy the built app into the installation directory |
| `open`, `--open`, `--launch` | Install and launch the app |
| `no-build`, `--no-build` | Install an already-built bundle |
| `--path <dir>` | Override the installation directory |
| `-h`, `--help`, `help` | Print build help |

Examples:

```bash
pnpm build release app
pnpm build debug install open
pnpm build --no-build --path "$HOME/Applications"
```

## Configuration

### Settings and private state

Use the Settings UI for normal configuration. The backend persists non-secret settings and queue state in Tauri's app-data directory for `com.chann.wakenote`, normally:

```text
~/Library/Application Support/com.chann.wakenote/
```

| File | Purpose |
| --- | --- |
| `settings.json` | Non-secret application settings |
| `transcription-queue.json` | Durable queue and Activity state |
| `dictionary.txt` | Human-editable shared Dictionary |
| `openrouter-secrets.json` | OpenRouter API key |
| `openai-secrets.json` | OpenAI API key |
| `soniox-secrets.json` | Soniox API key |
| `list-visibility.json` | Hidden-item state without deleting source files |
| `transcription-costs.json` | Bounded provider usage and local estimates |

Do not put API keys in `settings.json`. The frontend receives only a configured/missing boolean for each provider.

### Capture defaults

Settings patches are clamped or normalized by the Rust backend.

| Setting | Default | Supported range or values |
| --- | --- | --- |
| Recording | `on` | `on` / `off` |
| Transcription | `on` | `on` / `off` |
| Language | `ko` | `auto`, `ko`, `en`, `ja`, `zh`, `es`, `fr`, `de` |
| Primary microphone | `System Default` | one available input |
| Secondary microphone | none | one distinct physical input |
| Microphone processing | Merge Audio | Separate recordings / Merge Audio / Priority Audio |
| Threshold | `-40 dBFS` | `-90 … -10` |
| Voice-aware Auto Level | `on` | Local RNN speech detection, adaptive digital gain, clipping protection, and hardware input-volume recommendations when the microphone allows them |
| Mic input volume | Current macOS value | `0 … 100%`; system-authoritative and not reset with recording defaults |
| Attack | `200 ms` | `50 … 2,000 ms` |
| Release | `1,000 ms` | `250 … 5,000 ms` |
| Pre-roll | `400 ms` | `0 … 1,500 ms` |
| Lead-in | `200 ms` | `0 … 2,000 ms` |
| Post-roll | `400 ms` | `0 … 2,000 ms` |
| Min Chunk | `800 ms` | `100 … 5,000 ms` |
| Max Chunk | `180,000 ms` | `10,000 … 900,000 ms` |
| Audio format | `m4a` | `m4a`, `mp3`, `wav` |
| Compressed bitrate | `96 kbps` | `64`, `96`, `128 kbps`; ignored for WAV |
| Save root | `~/Documents/WakeNote` | user-selected directory |
| Selected model | `whisper-medium` | compatible ready model |
| Model directory | `~/Library/Application Support/WakeNote/models` | user-selected directory |
| Shared Dictionary | `on` | `on` / `off` |
| Hide low-confidence transcripts | `on` | `on` / `off` |
| Start live input on launch | `on` | `on` / `off` |
| Input monitoring | `off` | `on` / `off`; Primary only |
| Shortcut dictation | `off` | `on` / `off` |
| Dictation shortcut | `Option+Space` | supported key, key chord, or physical modifier combination |
| Dictation language | `auto` | same language set as archival transcription |
| Subtitle | `off` | `on` / `off` |
| Subtitle position | top center | off; top/bottom × left/center/right |
| Subtitle font size | `24 px` | `10 … 48 px` |
| Subtitle background | black at `82%` | black/white, `0 … 100%` |
| Subtitle horizontal padding | `18 px` | `0 … 64 px` |
| Subtitle vertical padding | `14 px` | `0 … 64 px` |
| Subtitle border | `1 px`, `#ffffff` | `0 … 8 px`, six-digit hex color |
| Subtitle corner radius | `8 px` | `0 … 48 px` |
| Subtitle maximum width | `720 px` | `0 … 1,600 px` |
| Subtitle height | `58 … 1,000 px` | minimum/maximum `0 … 1,200 px`; normalized so minimum ≤ maximum |
| Minimum subtitle duration | `5 sec` | `1 … 10 sec` |
| Subtitle animation | fade | instant / fade / dissolve |
| Theme | dark | light / dark |
| OpenRouter report model | `z-ai/glm-5.2` | provider model ID |
| Maximum report iterations | `3` | `1 … 30` |

### Model capabilities

The model picker filters by context. A model that is valid for a saved file is not automatically valid for realtime preview or diarization.

| WakeNote model | Runtime | Selectable contexts | Setup |
| --- | --- | --- | --- |
| Whisper Small / Medium / Turbo / Large | `whisper-rs` with Metal on macOS | File, Dictation, Meeting | Download and verify in Models |
| Parakeet TDT 0.6B V3 | bundled sherpa-onnx | File, Dictation, Meeting | Download and verify in Models |
| SenseVoice Small | bundled sherpa-onnx | File, Dictation, Meeting | Download and verify in Models |
| Nemotron 3.5 ASR Streaming 0.6B | bundled sherpa-onnx | File, Realtime, Dictation, Meeting | Download and verify in Models |
| Qwen3-ASR 0.6B / 1.7B | isolated Transformers runtime | File, Dictation, Meeting | Run **Set up Qwen3-ASR** in Models |
| OpenRouter · Qwen3 ASR Flash | `qwen/qwen3-asr-flash-2026-02-10` | File, Dictation, Meeting | OpenRouter key |
| OpenAI · GPT Transcribe | `gpt-transcribe` | File, Realtime, Dictation, Meeting | OpenAI key |
| OpenAI · GPT Live Transcribe | `gpt-live-transcribe` | Realtime, Dictation | OpenAI key; streaming required |
| OpenAI · GPT-4o Transcribe Diarize | `gpt-4o-transcribe-diarize` | File, Meeting | OpenAI key; supports speaker separation |
| Soniox · Async V5 | `stt-async-v5` | File, Dictation, Meeting | Soniox key |
| Soniox · Real-time V5 | `stt-rt-v5` | Realtime, Dictation | Soniox key; streaming required |

Plain `cargo test` leaves the optional `asr-sherpa` feature disabled to keep local Rust iteration lighter. The repository packaging path enables and bundles it automatically.

### Cloud credentials and network behavior

Save or delete keys in **Settings › Integrations › External AI API keys**:

| Provider | Used for |
| --- | --- |
| OpenRouter | Qwen3 ASR Flash, transcript summaries, and detailed reports |
| OpenAI | GPT file, realtime, and diarization transcription |
| Soniox | Async V5 and Real-time V5 transcription |

Selecting a cloud model sends audio and supported context to that provider. Connectivity, retention, rate limits, privacy, and billing follow the provider account. Missing keys and classified provider failures remain visible on the selected job; WakeNote does not silently switch cloud providers.

**Cost-limit fallback** is opt-in. For a cloud model, choose an installed compatible on-device model and enable the switch. WakeNote retries once only when the primary API response is classified as an exhausted balance or spend limit. Other authentication, provider, transport, decoding, and content failures do not trigger fallback. Requested, effective, and fallback model IDs are stored in provenance metadata.

### Shared Dictionary

Use **Settings › Models › Dictionary** or edit `dictionary.txt` through the provided action. The text format is:

```text
# Canonical term = aliases
WakeNote = wake note, 웨이크 노트
Qwen3-ASR = qwen 3 asr
Soniox
```

Blank lines and `#` comments are ignored. The file accepts at most 1 MiB and 1,000 active lines. A malformed edit reports the line and keeps the last valid Dictionary active.

WakeNote sends canonical terms as native provider context where supported, then performs deterministic alias-to-canonical correction before saving, captioning, or typing. ASCII aliases match case-insensitively at alphanumeric boundaries; non-ASCII aliases match exactly. There is no fuzzy replacement.

### Subtitle presentation

Enable **Settings › Integrations › Subtitle** and choose any top/bottom × left/center/right position. Corner positions use a smaller display-edge margin than the recording waveform, while the centered positions keep their existing anchor.

Padding, border width/color/radius, maximum width, and minimum/maximum height are numeric pixel settings. Short subtitles remain content-sized; long subtitles never grow beyond the configured maximum width. The backend balances long lines from the current full caption, softly preferring sentence endings when the visual result is otherwise similar. The same explicit lines size the native window and render in the web content. Unbreakable URLs and long tokens remain intact and may wrap anywhere only as an overflow safeguard.

Changing a presentation option shows the real overlay as a preview. Each edit restarts the preview for at least the configured 1–10 second minimum. Longer live, dictation, and preview text stays visible for its estimated reading time, capped at 30 seconds. All caption sources share the same generation-safe expiry, so an older timer cannot hide newer text and a final subtitle does not remain on screen indefinitely.

### History navigation and Trash

**Transcripts** and **Activity** open newest-first and can switch to oldest-first. Both navigate one local calendar day at a time. Transcripts renders at most 50 filtered rows per page; changing the day, filter, or order returns to the first page and clears stale off-page selection. This bounds browser DOM and audio-control work, while the existing per-day `all.json` remains the disk index rather than a server-style cursor.

Activity's **Warnings & errors**, **Needs attention**, and **Resolved** views review outcomes rather than one day's work, so they gather every match across all dates and hide the date picker. Rows keep a day heading, the header checkbox selects every match across dates and pages, and Move to Trash acts on that whole selection.

Selected Transcript rows can be moved to Trash together. WakeNote stages every exact-stem audio, `.txt`, `.json`, and `.error.txt` sibling, rolls the bundle back if a move fails, then refreshes that day's `all.json`. Activity uses the same bundle transaction. Cancelling a running Activity job is idempotent and suppresses late partial/final writes; deleting one requests cancellation and waits for its worker to exit before moving the bundle and removing the queue record. Trash remains recoverable through macOS Finder.

### Shortcut dictation

Enable **Settings › Dictation › Shortcut dictation**, choose a compatible model and language, then keep the cursor in the destination application:

1. Hold the configured shortcut to start dedicated Primary-microphone capture.
2. Speak while holding it, then release it to stop and transcribe.
3. WakeNote writes the dictation artifact and, for non-empty final text, types at the focused cursor.

The dictation stream is independent from voice-activated archival capture. VOR keeps recording during Dictation, but overlapping VOR chunks stay audio-only and skip transcription to avoid saving the same speech twice. Each Dictation attempt is stored with a `dictation` source label. Very quiet or empty output is not typed, a press during transcription is ignored, and recording stops automatically after ten minutes. WakeNote restores the previous clipboard after native insertion.

The top-center feedback bubble remains available even when Subtitle is off. Microphone permission is required for capture; Accessibility permission is required for focused-cursor typing.

### Reports

Reports use OpenRouter and the configured report model. Start from **Reports › New report** or select captures in **Transcripts** and choose Summary or Report.

- Summary produces a concise Markdown structure with key points, decisions, actions, and open questions.
- Detailed report adds context, chronology, risks, and evidence notes, with grading/refinement up to the configured iteration limit.
- Only one report run is active at a time. Active work can be cancelled; failed or cancelled runs can be retried.
- Report Markdown and metadata remain under `<save_root>/reports/`. Hiding a report changes list visibility and does not delete files.

Prompt templates in **Settings › Integrations › OpenRouter reports** support `{{transcripts}}`, `{{date_range}}`, and `{{selected_count}}`.

### Environment variables

| Variable | Purpose |
| --- | --- |
| `WAKENOTE_INSTALL_PATH` | Default app installation directory for the build script |
| `WAKENOTE_QWEN3_ASR_UV` | Explicit `uv` executable for Qwen setup |
| `WAKENOTE_QWEN3_ASR_PYTHON` | Explicit Python executable for Qwen setup |

Advanced external-command models receive runtime variables from WakeNote rather than reading global configuration. See the example below.

## Examples

### Develop the workspace without rebuilding Rust

```bash
pnpm dev
```

The browser fixture includes captures, reports, runs, queue outcomes, and installed models so the main UI is populated. It does not exercise native behavior.

### Keep two microphone identities separate

1. Select explicit, distinct Primary and Secondary devices in **Settings › Audio**.
2. Choose **Separate recordings** under **Microphone processing**.
3. Record normally.

WakeNote writes separate source-labelled files and interleaves their transcript entries chronologically. Input monitoring still uses Primary only.

### Import and transcribe a meeting

1. Open **Meetings** and import an `mp3`, `m4a`, or `wav` file.
2. Choose a model that supports Meeting.
3. Enable speaker separation only when the selected model advertises diarization.
4. Start transcription and leave the source file in place until WakeNote has copied it into the meeting directory.

Progress is checkpointed to `meeting.json` after each segment so interrupted work can resume.

Manual Meeting recording is streamed through a hidden temporary WAV and finalized as `audio.m4a` with the configured compressed bitrate. The temporary file is removed after successful encoding. If finalization fails, WakeNote keeps recoverable audio instead of discarding the recording. Imported meeting files keep their original `mp3`, `m4a`, or `wav` codec.

### Add an external-command model

Place a command in `<model_directory>/<model-id>.command`. WakeNote passes:

- `WAKENOTE_AUDIO_PATH`
- `WAKENOTE_MODEL_ID`
- `WAKENOTE_MODEL_DIRECTORY`
- `WAKENOTE_LANGUAGE`
- `WAKENOTE_DICTIONARY_TERMS`
- `WAKENOTE_DICTIONARY_JSON`

The command must write only the final transcript to stdout and exit successfully. Treat every path and Dictionary value as untrusted input, and never print credentials.

### Verify a packaged DMG

After `pnpm dmg:downloads`:

```bash
hdiutil verify "$HOME/Downloads/WakeNote_<version>_aarch64.dmg"
shasum -a 256 "$HOME/Downloads/WakeNote_<version>_aarch64.dmg"
```

Use the actual generated filename for `<version>`.

## Troubleshooting

### The app cannot hear a microphone

- Confirm **Settings › Audio › Microphone Permission** is granted.
- Verify that the pinned device is present. WakeNote reconnects to the same device rather than silently replacing it with another physical input.
- In a two-microphone setup, choose two explicit, distinct devices; do not combine `System Default` with a Secondary input.
- Watch each microphone status row for Active, Waiting, or Reconnecting state.
- The input-volume row mirrors macOS. `Digital auto level only` means the device exposes no writable hardware volume; capture remains available and the row is intentionally read-only.
- Some USB interfaces manage gain with a physical knob and do not expose a macOS input-volume property.
- With **Voice-aware Auto Level** enabled, WakeNote detects speech locally and may move a writable macOS input slider during capture. Primary and Secondary microphones are adjusted independently.
- `Hardware + digital auto level` combines macOS input-volume recommendations with adaptive digital gain. `Digital auto level only` keeps the local digital protection when the device is read-only.
- Disable **Voice-aware Auto Level** to use `Manual system volume`. Samples already clipped by the microphone's analog-to-digital converter cannot be reconstructed; lower the physical gain when clipping persists.

### The level moves but no chunk starts

- Use Calibration and compare the speech level with the noise floor.
- A more negative Threshold is more sensitive and may capture background noise.
- Confirm Recording is on and Pause is off.
- Check Attack and Min Chunk if short utterances are missing.

### A queue job stays pending

- Confirm Transcription is on and Pause is off.
- Open **Settings › Models**. Local jobs wait when their selected model is not installed or verified.
- Open **Settings › Integrations**. Cloud jobs require the matching provider key.
- Activity jobs retain the model chosen when they were queued; selecting a different global model does not rewrite existing jobs. Use reprocessing when you intend to choose a new model.

### Activity shows a warning instead of an error

No-speech, empty-transcript, low-confidence, and transcript-artifact outcomes are completed quality warnings. They preserve the recording and remain available for review or model-specific reprocessing. **Mark all resolved** acknowledges them without deleting history.

### A running Activity job does not disappear immediately after Delete

Deletion waits until the active transcription worker acknowledges cancellation so it cannot recreate a transcript or metadata sidecar after the bundle reaches Trash. A non-interruptible cloud request may therefore leave the row visible briefly. Do not force-quit the app during that handoff; WakeNote removes the queue record only after worker exit and a successful recoverable bundle move.

### Cloud transcription failed

- Verify the matching provider key and account status without pasting the key into logs or issue reports.
- Check network connectivity, provider rate limits, request-size limits, and billing state.
- Retry explicitly from Activity after correcting the cause.
- WakeNote preserves the selected provider on ordinary failures. Only an enabled on-device cost-limit fallback handles a classified billing-limit response.

### Dictation records but does not type

- Grant **Accessibility** permission for WakeNote/System Events in **System Settings › Privacy & Security › Accessibility**.
- Keep the destination cursor focused until transcription finishes.
- Empty or low-signal results are deliberately not inserted.
- Shortcut events received while a previous dictation is transcribing are ignored.

### MP3 recording fails

Install `ffmpeg` and ensure it is available in `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, or `PATH`. MP3 encoding uses `libmp3lame`. Select M4A or WAV if MP3 is not required.

### M4A recording fails

Confirm `/usr/bin/afconvert` is available. It is provided by macOS/Xcode Command Line Tools. WAV bypasses the native compressed-audio encoder.

### The browser UI works but native behavior does not

`pnpm dev` uses fixtures. Reproduce through `pnpm tauri dev` or an installed app before drawing conclusions about microphones, ScreenCaptureKit, filesystem writes, permissions, the tray, focused-cursor insertion, or real provider calls.

### DMG packaging reports an unknown argument

Run `pnpm dmg:downloads` directly. `pnpm build dmg:downloads` is invalid because `dmg:downloads` is a package script, not a build CLI argument.

### Known limitations

- The product and packaging workflow are macOS-specific.
- MP3 depends on `ffmpeg`; M4A depends on `/usr/bin/afconvert`.
- Large Whisper and Qwen models can be slow or memory-heavy on lower-end Macs.
- Cloud models send audio to their provider and are subject to external availability, retention, rate-limit, and billing policies.
- Browser fixtures prove frontend behavior only.
- No project license has been specified yet.
