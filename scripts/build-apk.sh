#!/usr/bin/env bash
# Build a .papk file for a given Java example app.
#
# Usage:
#   ./scripts/build-apk.sh --app helloworld
#   ./scripts/build-apk.sh --app blinky --output /tmp/blinky.papk
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
APP=""
OUTPUT=""
HOST_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --app    <app>    Example app to build: helloworld, blinky, uart, arraydemo,
                    inherit, interfacedemo, floatdemo, exceptiondemo, threaddemo,
                    mathsdemo, i2cdemo, spidemo
  --output <file>   Output path (default: build/apks/<app>.papk)
  -h, --help        Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)  usage; exit 0 ;;
    --app)      APP="$2";    shift 2 ;;
    --output)   OUTPUT="$2"; shift 2 ;;
    *)          echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$APP" ]]; then
  echo "Error: --app is required" >&2
  usage
  exit 1
fi

APP_DIR="$REPO_ROOT/java/examples/$APP"
MANIFEST_FILE="$APP_DIR/PicodroidManifest.xml"

if [[ ! -d "$APP_DIR" ]]; then
  echo "Error: app directory not found: $APP_DIR" >&2
  exit 1
fi

if [[ ! -f "$MANIFEST_FILE" ]]; then
  echo "Error: PicodroidManifest.xml not found in $APP_DIR" >&2
  exit 1
fi

# Read main-class and version from PicodroidManifest.xml
MAIN_CLASS=$(python3 - "$MANIFEST_FILE" <<'EOF'
import sys, xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
print(root.find("application").get("main-class", ""))
EOF
)

VERSION=$(python3 - "$MANIFEST_FILE" <<'EOF'
import sys, xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
print(root.get("version", "1.0"))
EOF
)

if [[ -z "$MAIN_CLASS" ]]; then
  echo "Error: 'main-class' not found in $MANIFEST_FILE" >&2
  exit 1
fi

OUTPUT="${OUTPUT:-$REPO_ROOT/build/apks/${APP}.papk}"
CLASSES_DIR="$REPO_ROOT/build/classes/$APP"
FRAMEWORK_CLASSES_DIR="$REPO_ROOT/build/classes/framework"

# Clean the app classes dir each build to avoid stale framework classes from
# older builds (before the framework was separated into its own directory).
rm -rf "$CLASSES_DIR"
mkdir -p "$CLASSES_DIR" "$FRAMEWORK_CLASSES_DIR" "$REPO_ROOT/build/apks"

# Step 1: Compile picodroid framework sources into a separate classes directory.
# This mirrors Android's model: framework classes are part of the platform, not the APK.
# They are embedded into firmware by build.rs; here we only need them as a classpath.
FRAMEWORK_JAVA_FILES=()
while IFS= read -r -d '' f; do
  FRAMEWORK_JAVA_FILES+=("$f")
done < <(find "$REPO_ROOT/java/framework/java" -name "*.java" -print0)

echo "==> Compiling picodroid framework..."
javac --release 8 \
  -d "$FRAMEWORK_CLASSES_DIR" \
  "${FRAMEWORK_JAVA_FILES[@]}"

# Step 2: Compile only app sources, using compiled framework classes as classpath.
APP_JAVA_FILES=()
while IFS= read -r -d '' f; do
  APP_JAVA_FILES+=("$f")
done < <(find "$APP_DIR" -name "*.java" -print0)

echo "==> Compiling Java sources for '$APP'..."
javac --release 8 \
  -cp "$FRAMEWORK_CLASSES_DIR" \
  -d "$CLASSES_DIR" \
  "${APP_JAVA_FILES[@]}"

echo "==> Packaging '$APP' into $(basename "$OUTPUT")..."
cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$REPO_ROOT/tools/papk-pack/Cargo.toml" \
  -- \
  --main-class "$MAIN_CLASS" \
  --package-name "$APP" \
  --version "$VERSION" \
  --classes-dir "$CLASSES_DIR" \
  --output "$OUTPUT"
