#!/usr/bin/env bash
# Patch permissions into the generated AndroidManifest after `tauri android init`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/gen/android/app/src/main/AndroidManifest.xml"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: $MANIFEST not found; run \`tauri android init\` first" >&2
  exit 1
fi

python3 - "$MANIFEST" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

permissions = [
    "android.permission.INTERNET",
    "android.permission.WAKE_LOCK",
    "android.permission.FOREGROUND_SERVICE",
    "android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK",
    "android.permission.MODIFY_AUDIO_SETTINGS",
    "android.permission.POST_NOTIFICATIONS",
    "android.permission.READ_MEDIA_AUDIO",
    "android.permission.READ_EXTERNAL_STORAGE",
]

inserts = []
for perm in permissions:
    marker = f'android:name="{perm}"'
    if marker in text:
        continue
    if perm == "android.permission.READ_EXTERNAL_STORAGE":
        inserts.append(
            f'    <uses-permission android:name="{perm}" android:maxSdkVersion="32" />'
        )
    else:
        inserts.append(f'    <uses-permission android:name="{perm}" />')

if not inserts:
    print(f"Android permissions already present in {path}")
    sys.exit(0)

block = "\n".join(inserts) + "\n"

# Prefer inserting before the first <application ...> tag.
app_idx = text.find("<application")
if app_idx == -1:
    print("error: could not find <application> in AndroidManifest.xml", file=sys.stderr)
    sys.exit(1)

text = text[:app_idx] + block + text[app_idx:]
path.write_text(text)
print(f"Patched Android permissions into {path}")
for line in inserts:
    print(f"  + {line.strip()}")
PY
