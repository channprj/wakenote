#!/bin/bash
# The caller has already downloaded, hashed, mounted and staged the new app.
# All paths are positional arguments; PATH is fixed by the native launcher.
set -u
APP_PID="$1"
DEST="$2"
VERSION="$3"
ROOT="$(cd -- "$(dirname -- "$0")" && pwd -P)" || exit 2
STAGED="$ROOT/WakeNote.app"
BACKUP="$ROOT/previous.app"

valid_app() {
  [ -d "$1" ] && [ ! -L "$1" ] &&
    [ "$(plutil -extract CFBundleIdentifier raw -o - "$1/Contents/Info.plist")" = "com.chann.wakenote" ] &&
    [ "$(plutil -extract CFBundleShortVersionString raw -o - "$1/Contents/Info.plist")" = "$VERSION" ] &&
    codesign --verify --deep --strict "$1" >/dev/null 2>&1
}

fail() {
  trap - INT TERM HUP
  if [ -d "$BACKUP" ]; then
    # Never discard the only old copy, including when restoring a swap fails.
    if [ -e "$DEST" ]; then mv "$DEST" "$ROOT/failed.app" 2>/dev/null || exit 1; fi
    mv "$BACKUP" "$DEST" 2>/dev/null || exit 1
  fi
  osascript -e 'display notification "Update failed. Your existing app was kept." with title "WakeNote"' >/dev/null 2>&1 || true
  [ -d "$DEST" ] && open "$DEST" >/dev/null 2>&1 || true
  [ ! -e "$BACKUP" ] && rm -rf "$ROOT"
  exit 1
}

case "$APP_PID" in ''|*[!0-9]*) exit 2 ;; esac
[ "$APP_PID" -gt 1 ] && [ -d "$DEST" ] && [ ! -L "$DEST" ] || exit 2
[ "$(dirname -- "$DEST")" = "$(dirname -- "$ROOT")" ] || exit 2
valid_app "$STAGED" || fail
trap fail INT TERM HUP

# A hung shutdown must never replace a bundle still in use.
attempt=0
while kill -0 "$APP_PID" 2>/dev/null; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 300 ]; then
    trap - INT TERM HUP
    rm -rf "$ROOT"
    exit 1
  fi
  sleep 0.2
done

mv "$DEST" "$BACKUP" || fail
mv "$STAGED" "$DEST" || fail
valid_app "$DEST" || fail
open "$DEST" || fail
trap - INT TERM HUP
rm -rf "$ROOT"
