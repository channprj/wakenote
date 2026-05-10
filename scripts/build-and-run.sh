#!/usr/bin/env bash
# Build the Tauri app locally and launch it immediately.
# Usage:
#   scripts/build-and-run.sh            # release build (default)
#   scripts/build-and-run.sh --debug    # debug build
#   scripts/build-and-run.sh --no-run   # build only, do not launch

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

MODE="release"
RUN_AFTER_BUILD=1

running_wakenote_process() {
  pgrep -x sagwan >/dev/null 2>&1 || pgrep -x wakenote >/dev/null 2>&1
}

quit_running_wakenote() {
  if ! running_wakenote_process; then
    return
  fi

  echo "==> Quitting running WakeNote"
  osascript -e 'tell application id "com.chann.wakenote" to quit' >/dev/null 2>&1 || true
  osascript -e 'tell application id "com.chann.sagwan" to quit' >/dev/null 2>&1 || true

  for _ in {1..20}; do
    if ! running_wakenote_process; then
      return
    fi
    sleep 0.5
  done

  echo "error: WakeNote is still running. Quit it and rerun this script." >&2
  exit 1
}

for arg in "$@"; do
  case "${arg}" in
    --debug) MODE="debug" ;;
    --release) MODE="release" ;;
    --no-run) RUN_AFTER_BUILD=0 ;;
    -h|--help)
      sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "error: unknown argument: ${arg}" >&2
      exit 2
      ;;
  esac
done

for cmd in pnpm cargo; do
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "error: '${cmd}' is required but not found in PATH" >&2
    exit 1
  fi
done

if [[ ! -d node_modules ]]; then
  echo "==> Installing JS dependencies (pnpm install)"
  pnpm install
fi

if [[ "${MODE}" == "debug" ]]; then
  echo "==> Building Tauri app (debug)"
  pnpm tauri build --debug
else
  echo "==> Building Tauri app (release)"
  pnpm tauri build
fi

APP_BUNDLE_RELEASE="${PROJECT_ROOT}/src-tauri/target/release/bundle/macos/WakeNote.app"
APP_BUNDLE_DEBUG="${PROJECT_ROOT}/src-tauri/target/debug/bundle/macos/WakeNote.app"
BIN_RELEASE="${PROJECT_ROOT}/src-tauri/target/release/wakenote"
BIN_DEBUG="${PROJECT_ROOT}/src-tauri/target/debug/wakenote"

if [[ "${MODE}" == "debug" ]]; then
  APP_BUNDLE="${APP_BUNDLE_DEBUG}"
  BIN="${BIN_DEBUG}"
else
  APP_BUNDLE="${APP_BUNDLE_RELEASE}"
  BIN="${BIN_RELEASE}"
fi

if [[ "${RUN_AFTER_BUILD}" -eq 0 ]]; then
  echo "==> Build complete. Skipping launch (--no-run)."
  [[ -d "${APP_BUNDLE}" ]] && echo "    bundle: ${APP_BUNDLE}"
  [[ -x "${BIN}" ]] && echo "    binary: ${BIN}"
  exit 0
fi

echo "==> Launching app"
if [[ "$(uname -s)" == "Darwin" && -d "${APP_BUNDLE}" ]]; then
  quit_running_wakenote
  open "${APP_BUNDLE}"
elif [[ -x "${BIN}" ]]; then
  exec "${BIN}"
else
  echo "error: could not locate built app at:" >&2
  echo "  - ${APP_BUNDLE}" >&2
  echo "  - ${BIN}" >&2
  exit 1
fi
