#!/usr/bin/env bash
# Wire Tauri Android release signing after `tauri android init`.
#
# Required env:
#   ANDROID_KEY_BASE64  – base64 of the .jks / .keystore
#   ANDROID_KEY_ALIAS   – key alias
#   ANDROID_KEY_PASSWORD – key password (also used as store password if
#                          ANDROID_STORE_PASSWORD is unset)
#
# Optional:
#   ANDROID_STORE_PASSWORD – store password when different from key password
#   RUNNER_TEMP            – directory for the decoded keystore (CI default)
#
# Writes:
#   src-tauri/gen/android/keystore.properties
# and patches:
#   src-tauri/gen/android/app/build.gradle.kts
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_DIR="$ROOT/src-tauri/gen/android"
GRADLE_FILE="$ANDROID_DIR/app/build.gradle.kts"

if [[ ! -d "$ANDROID_DIR" ]]; then
  echo "error: $ANDROID_DIR not found; run \`tauri android init\` first" >&2
  exit 1
fi

if [[ ! -f "$GRADLE_FILE" ]]; then
  echo "error: $GRADLE_FILE not found" >&2
  exit 1
fi

: "${ANDROID_KEY_BASE64:?ANDROID_KEY_BASE64 is required}"
: "${ANDROID_KEY_ALIAS:?ANDROID_KEY_ALIAS is required}"
: "${ANDROID_KEY_PASSWORD:?ANDROID_KEY_PASSWORD is required}"

STORE_PASSWORD="${ANDROID_STORE_PASSWORD:-$ANDROID_KEY_PASSWORD}"
KEYSTORE_DIR="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
KEYSTORE_PATH="$KEYSTORE_DIR/wave-upload-keystore.jks"

echo "$ANDROID_KEY_BASE64" | tr -d '\n\r ' | base64 --decode >"$KEYSTORE_PATH"

if [[ ! -s "$KEYSTORE_PATH" ]]; then
  echo "error: decoded keystore is empty; check ANDROID_KEY_BASE64" >&2
  exit 1
fi

# Properties values are written without escaping; avoid passwords containing
# newlines. Prefer a single shared password via the `password` key (Tauri docs).
{
  printf 'keyAlias=%s\n' "$ANDROID_KEY_ALIAS"
  printf 'storeFile=%s\n' "$KEYSTORE_PATH"
  if [[ "$STORE_PASSWORD" == "$ANDROID_KEY_PASSWORD" ]]; then
    printf 'password=%s\n' "$ANDROID_KEY_PASSWORD"
  else
    printf 'storePassword=%s\n' "$STORE_PASSWORD"
    printf 'keyPassword=%s\n' "$ANDROID_KEY_PASSWORD"
  fi
} >"$ANDROID_DIR/keystore.properties"

echo "Wrote $ANDROID_DIR/keystore.properties"

python3 - "$GRADLE_FILE" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text()

if "signingConfigs" in text and 'getByName("release")' in text and "signingConfig = signingConfigs.getByName(\"release\")" in text:
    print(f"Signing already configured in {path}")
    sys.exit(0)

if "import java.io.FileInputStream" not in text:
    if "import java.util.Properties" in text:
        text = text.replace(
            "import java.util.Properties",
            "import java.io.FileInputStream\nimport java.util.Properties",
            1,
        )
    else:
        # Insert after the last existing import, or at the top.
        lines = text.splitlines(keepends=True)
        insert_at = 0
        for i, line in enumerate(lines):
            if line.startswith("import "):
                insert_at = i + 1
        lines.insert(insert_at, "import java.io.FileInputStream\n")
        if "import java.util.Properties" not in text:
            lines.insert(insert_at + 1, "import java.util.Properties\n")
        text = "".join(lines)

signing_block = """
    signingConfigs {
        create("release") {
            val keystorePropertiesFile = rootProject.file("keystore.properties")
            val keystoreProperties = Properties()
            if (keystorePropertiesFile.exists()) {
                keystoreProperties.load(FileInputStream(keystorePropertiesFile))
            }

            keyAlias = keystoreProperties["keyAlias"] as String
            keyPassword = (
                keystoreProperties["keyPassword"] ?: keystoreProperties["password"]
            ) as String
            storeFile = file(keystoreProperties["storeFile"] as String)
            storePassword = (
                keystoreProperties["storePassword"] ?: keystoreProperties["password"]
            ) as String
        }
    }
"""

if "signingConfigs" not in text:
    marker = "    buildTypes {"
    if marker not in text:
        print("error: could not find buildTypes block in", path, file=sys.stderr)
        sys.exit(1)
    text = text.replace(marker, signing_block + "\n" + marker, 1)

release_marker = '        getByName("release") {'
if 'signingConfig = signingConfigs.getByName("release")' not in text:
    if release_marker not in text:
        print("error: could not find release buildType in", path, file=sys.stderr)
        sys.exit(1)
    text = text.replace(
        release_marker,
        release_marker + '\n            signingConfig = signingConfigs.getByName("release")',
        1,
    )

path.write_text(text)
print(f"Patched signing config into {path}")
PY
