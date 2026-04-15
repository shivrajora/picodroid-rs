#!/usr/bin/env bash
# Build a .papk file for a given Java example app.
#
# Usage:
#   ./scripts/build-apk.sh -a helloworld
#   ./scripts/build-apk.sh -a blinky -o /tmp/blinky.papk
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"
APP=""
OUTPUT=""
HOST_TARGET="$(host_target)"

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -a, --app    <app>    Example app to build
  -o, --output <file>   Output path (default: build/apks/<app>.papk)
      --shrink          Apply the active release shrink map (class-name
                        shrinking). Off by default; also honored via
                        PICODROID_SHRINK=1. See docs/shrinker.md.
  -h, --help            Show this help message

Apps:
$(list_apps "$REPO_ROOT/examples")
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)       usage; exit 0 ;;
    -a|--app)        APP="$2";    shift 2 ;;
    -o|--output)     OUTPUT="$2"; shift 2 ;;
    --shrink)        export PICODROID_SHRINK=1; shift ;;
    *)          echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$APP" ]]; then
  echo "Error: --app is required" >&2
  usage
  exit 1
fi

APP_DIR="$REPO_ROOT/examples/$APP"
MANIFEST_FILE="$APP_DIR/PicodroidManifest.xml"

if [[ ! -d "$APP_DIR" ]]; then
  echo "Error: app directory not found: $APP_DIR" >&2
  exit 1
fi

if [[ ! -f "$MANIFEST_FILE" ]]; then
  echo "Error: PicodroidManifest.xml not found in $APP_DIR" >&2
  exit 1
fi

# Read main-class, activity, application, and version from PicodroidManifest.xml in one pass.
mapfile -t _manifest_attrs < <(python3 - "$MANIFEST_FILE" <<'EOF'
import sys, xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
app = root.find("application")
print(app.get("main-class", ""))
print(app.get("activity", ""))
print(app.get("application", ""))
print(root.get("version", "1.0"))
EOF
)
MAIN_CLASS="${_manifest_attrs[0]}"
ACTIVITY="${_manifest_attrs[1]}"
APPLICATION="${_manifest_attrs[2]}"
VERSION="${_manifest_attrs[3]}"

if [[ -z "$MAIN_CLASS" && -z "$ACTIVITY" && -z "$APPLICATION" ]]; then
  echo "Error: either 'main-class', 'activity', or 'application' must be set in $MANIFEST_FILE" >&2
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
done < <(find "$REPO_ROOT/sdk/java" -name "*.java" -print0)

echo "==> Compiling picodroid framework..."
javac --release 8 -Xlint:-options \
  -d "$FRAMEWORK_CLASSES_DIR" \
  "${FRAMEWORK_JAVA_FILES[@]}"

# Step 2: Compile only app sources, using compiled framework classes as classpath.
APP_JAVA_FILES=()
while IFS= read -r -d '' f; do
  APP_JAVA_FILES+=("$f")
done < <(find "$APP_DIR" -name "*.java" -print0)

echo "==> Compiling Java sources for '$APP'..."
javac --release 8 -Xlint:-options \
  -cp "$FRAMEWORK_CLASSES_DIR" \
  -d "$CLASSES_DIR" \
  "${APP_JAVA_FILES[@]}"

echo "==> Packaging '$APP' into $(basename "$OUTPUT")..."

# Shrinking is opt-in via --shrink / PICODROID_SHRINK=1. Off by default.
# Without it we advertise the "0.0.0" sentinel and skip the rewrite step
# entirely. Firmware's build.rs honors the same env var so both sides
# agree on whether shrinking is active.
if [[ "${PICODROID_SHRINK:-}" == "1" ]]; then
  FRAMEWORK_MAP_VERSION="$(cargo run --quiet --target "$HOST_TARGET" \
    --manifest-path "$REPO_ROOT/tools/class-shrink/Cargo.toml" -- \
    print-version \
    --cargo-toml "$REPO_ROOT/Cargo.toml" \
    --shrink-maps-dir "$REPO_ROOT/sdk/shrink-maps")"
else
  FRAMEWORK_MAP_VERSION="0.0.0"
fi

# When a shrink map is active, rewrite the app's .class files so references
# to framework classes (e.g. `Lpicodroid/app/Application;`) get their
# shrunk forms. The app's own classes are NOT in the map, so they pass
# through unchanged. This keeps the main-class manifest value valid.
PACK_CLASSES_DIR="$CLASSES_DIR"
if [[ "$FRAMEWORK_MAP_VERSION" != "0.0.0" ]]; then
  MAP_PATH="$REPO_ROOT/sdk/shrink-maps/v${FRAMEWORK_MAP_VERSION}.toml"
  if [[ ! -f "$MAP_PATH" ]]; then
    echo "Error: active map $MAP_PATH resolved but file missing" >&2
    exit 1
  fi
  SHRUNK_DIR="$REPO_ROOT/build/classes/${APP}.shrunk"
  rm -rf "$SHRUNK_DIR"
  echo "==> Rewriting framework refs in '$APP' classes (map v$FRAMEWORK_MAP_VERSION)..."
  cargo run --quiet --target "$HOST_TARGET" \
    --manifest-path "$REPO_ROOT/tools/class-shrink/Cargo.toml" -- \
    shrink-dir --in "$CLASSES_DIR" --out "$SHRUNK_DIR" --map "$MAP_PATH"
  PACK_CLASSES_DIR="$SHRUNK_DIR"
fi

PAPK_ARGS=()
if [[ -n "$MAIN_CLASS" ]]; then
  PAPK_ARGS+=(--main-class "$MAIN_CLASS")
fi
if [[ -n "$ACTIVITY" ]]; then
  PAPK_ARGS+=(--activity "$ACTIVITY")
fi
if [[ -n "$APPLICATION" ]]; then
  PAPK_ARGS+=(--application "$APPLICATION")
fi

cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$REPO_ROOT/tools/papk-pack/Cargo.toml" \
  -- \
  "${PAPK_ARGS[@]}" \
  --package-name "$APP" \
  --version "$VERSION" \
  --framework-map-version "$FRAMEWORK_MAP_VERSION" \
  --classes-dir "$PACK_CLASSES_DIR" \
  --output "$OUTPUT"
