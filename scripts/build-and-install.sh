#!/usr/bin/env bash
# Build the Tauri app locally and install the resulting .app bundle.
# Usage:
#   scripts/build-and-install.sh                    # release build, install to /Applications
#   scripts/build-and-install.sh --path ~/Applications
#   scripts/build-and-install.sh --debug            # debug build
#   scripts/build-and-install.sh --no-build         # install an already-built bundle
#   scripts/build-and-install.sh --launch           # open the installed app after install

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

MODE="release"
INSTALL_PATH="/Applications"
DO_BUILD=1
LAUNCH_AFTER_INSTALL=0

quit_running_sagwan() {
  if ! pgrep -x '[sS]agwan' >/dev/null 2>&1; then
    return
  fi

  echo "==> Quitting running Sagwan"
  osascript -e 'tell application id "com.chann.sagwan" to quit' >/dev/null 2>&1 || true

  for _ in {1..20}; do
    if ! pgrep -x '[sS]agwan' >/dev/null 2>&1; then
      return
    fi
    sleep 0.5
  done

  echo "error: Sagwan is still running. Quit it and rerun this script." >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug) MODE="debug"; shift ;;
    --release) MODE="release"; shift ;;
    --path)
      [[ $# -ge 2 ]] || { echo "error: --path requires a value" >&2; exit 2; }
      INSTALL_PATH="$2"
      shift 2
      ;;
    --path=*) INSTALL_PATH="${1#--path=}"; shift ;;
    --no-build) DO_BUILD=0; shift ;;
    --launch) LAUNCH_AFTER_INSTALL=1; shift ;;
    -h|--help)
      sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: build-and-install.sh currently supports macOS only" >&2
  exit 1
fi

if [[ "${DO_BUILD}" -eq 1 ]]; then
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
fi

if [[ "${MODE}" == "debug" ]]; then
  APP_BUNDLE="${PROJECT_ROOT}/src-tauri/target/debug/bundle/macos/Sagwan.app"
else
  APP_BUNDLE="${PROJECT_ROOT}/src-tauri/target/release/bundle/macos/Sagwan.app"
fi

if [[ ! -d "${APP_BUNDLE}" ]]; then
  echo "error: bundle not found: ${APP_BUNDLE}" >&2
  echo "       run without --no-build, or build the app first." >&2
  exit 1
fi

INSTALL_PATH="${INSTALL_PATH/#\~/${HOME}}"

if [[ ! -d "${INSTALL_PATH}" ]]; then
  echo "==> Creating install directory: ${INSTALL_PATH}"
  mkdir -p "${INSTALL_PATH}"
fi

DEST="${INSTALL_PATH%/}/Sagwan.app"

quit_running_sagwan

SUDO=""
if [[ ! -w "${INSTALL_PATH}" ]]; then
  if command -v sudo >/dev/null 2>&1; then
    echo "==> Install path is not writable; using sudo for install"
    SUDO="sudo"
  else
    echo "error: install path '${INSTALL_PATH}' is not writable and sudo is unavailable" >&2
    exit 1
  fi
fi

if [[ -d "${DEST}" ]]; then
  echo "==> Removing existing bundle at ${DEST}"
  ${SUDO} rm -rf "${DEST}"
fi

echo "==> Installing to ${DEST}"
${SUDO} ditto "${APP_BUNDLE}" "${DEST}"

${SUDO} xattr -dr com.apple.quarantine "${DEST}" 2>/dev/null || true

echo "==> Done. Installed: ${DEST}"
echo "    Launch with: open '${DEST}'"

if [[ "${LAUNCH_AFTER_INSTALL}" -eq 1 ]]; then
  echo "==> Launching installed app"
  open "${DEST}"
fi
