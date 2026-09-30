# WakeNote

[한국어 README](README.ko.md)

**Local-first voice capture and transcription for macOS.**

WakeNote is a menu-bar recorder for one or two microphones, supported system-audio sources, imported recordings, and dictation with hold or toggle hotkeys. It turns speech into durable audio, metadata, transcript, and report files while letting you choose between on-device ASR and explicitly configured cloud providers.

The desktop app uses Tauri 2 and Rust for capture, persistence, and transcription, with a React 19 and TypeScript workspace for Capture, Meetings, Transcripts, Reports, Activity, and Settings.

## Highlights

- **Automatic capture** — a calibrated dBFS gate, attack/release timing, pre-roll, lead-in, and post-roll preserve speech without continuously writing silence.
- **Flexible dual microphones** — Keep Primary and Secondary separate, adaptively synchronize and merge them, or use Priority Audio to record whichever input is currently cleanest without summing both microphones.
- **Local and opt-in cloud ASR** — Whisper, sherpa-onnx, and Qwen3-ASR run locally; OpenRouter, OpenAI, and Soniox are available only after their API keys are saved.
- **Live and archival workflows** — Customizable Subtitles, independent hold and toggle dictation hotkeys, Recent Dictations with copy and history actions, a dedicated Dictations filter, paged daily transcript and Activity history, M4A meeting recordings, and Markdown reports share the same model and Dictionary contracts.
- **Focused-cursor tools** — Realtime auto-type has its own model selection. Opt-in OpenRouter translation supports Subtitles, Transcripts, and Dictation; a separate Enhanced Prompt shortcut turns spoken drafts into structured prompts with an editable system prompt.
- **Durable recovery** — queued jobs survive restarts, running work can be cancelled safely, and selected recording bundles move to macOS Trash together instead of leaving text, JSON, or audio behind.
- **Inspectable storage** — audio, metadata, transcripts, recoverable errors, meetings, and reports stay under a user-selected save root.

## Installation

WakeNote currently documents a source build. Install Node.js 20+, `pnpm` 10, the stable Rust toolchain, Xcode Command Line Tools, and CMake. On macOS, install CMake with `brew install cmake`, then run:

```bash
pnpm install
pnpm tauri dev
```

See [Usage](USAGE.md#installation) for macOS permissions, optional model prerequisites, release builds, local installation, and DMG packaging.

## Local releases

GitHub Actions is disabled for this repository. Publish from your Mac; the command reuses a verified current build or builds it first:

```bash
pnpm release:publish --dry-run
pnpm release:publish
```

Commit synchronized versions and push the branch and annotated version tag before publishing. `--dry-run` reports whether a build is needed without building or publishing. Use `pnpm release:build` to prepare artifacts separately. Artifacts go to `release/v<version>/<architecture>/`; each build targets the current Mac only. The app is ad-hoc signed and not notarized. See the [release procedure](USAGE.md#local-build-and-manual-github-release) for prerequisites, draft releases, and recovery.

## Quick start

1. Open **Settings › Audio**, allow Microphone access, and select the Primary input. Add a distinct Secondary input only when needed.
2. Open **Settings › Storage**, choose and confirm the save root.
3. Open **Settings › Models**, install or select a model. Add cloud keys only under **Settings › Integrations**.
4. Return to **Capture** and confirm that the live level crosses the configured threshold while speaking.
5. Review completed work in **Transcripts** and queue outcomes in **Activity**.

The default archive root is `~/Documents/WakeNote`; the default local model is `whisper-medium`.

## More documentation

- [Usage](USAGE.md) — installation, commands, configuration, model capabilities, examples, and troubleshooting
- [Architecture](ARCHITECTURE.md) — components, data flows, persistence boundaries, and design decisions

## License

WakeNote is licensed under the [MIT License](LICENSE). Bundled third-party components retain their own licenses.
