#!/usr/bin/env bash
# Compatibility wrapper for the pnpm build install flow.
# Prefer: pnpm build install [open] [debug]
# Usage:
#   scripts/build-and-install.sh                    # release build, install to /Applications
#   scripts/build-and-install.sh --path ~/Applications
#   scripts/build-and-install.sh --debug            # debug build
#   scripts/build-and-install.sh --no-build         # install an already-built bundle
#   scripts/build-and-install.sh --open             # open the installed app after install
#   scripts/build-and-install.sh --launch           # alias for --open

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

exec node scripts/build.mjs install "$@"
