#!/usr/bin/env bash
# Copy Wave-owned Java sources into the generated Android tree after
# `tauri android init` (which regenerates gen/android and would otherwise
# drop FolderPickerCallback / MediaNativeBridge).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src-tauri/android-src/java"
DEST="$ROOT/src-tauri/gen/android/app/src/main/java"

if [[ ! -d "$SRC" ]]; then
  echo "error: missing $SRC" >&2
  exit 1
fi

if [[ ! -d "$DEST" ]]; then
  echo "error: $DEST not found; run \`tauri android init\` first" >&2
  exit 1
fi

# Mirror package directories under android-src/java into the app java root.
cp -R "$SRC"/. "$DEST"/
echo "Synced Android Java sources:"
find "$SRC" -type f -name '*.java' | while read -r f; do
  rel="${f#$SRC/}"
  echo "  + $rel"
done
