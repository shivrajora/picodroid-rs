#!/usr/bin/env bash
# Build a PAPK whose framework-map-version is "from the future" relative
# to the running firmware, by synthesizing a v0.<MAJOR+1>.0.toml shrink
# map alongside the committed v0.1.0.toml.
#
# Used by hil-run.sh's `install-reject-future` rows. The synthetic map
# is a verbatim copy of the highest committed map (only the file name's
# semver is bumped), so it stays append-only and PAPKs built against it
# reference the same shrunk class names.
#
# Usage:
#   ./scripts/test-future-version-rejection.sh <app> <output.papk>
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP="${1:?usage: test-future-version-rejection.sh <app> <output.papk>}"
OUTPUT="${2:?usage: test-future-version-rejection.sh <app> <output.papk>}"

MAPS_DIR="$REPO_ROOT/sdk/shrink-maps"
LATEST_MAP=$(ls "$MAPS_DIR"/v*.toml 2>/dev/null \
              | sort -V | tail -1 || true)
if [[ -z "$LATEST_MAP" ]]; then
  echo "error: no committed maps under $MAPS_DIR — cannot synthesize a future map" >&2
  exit 1
fi

# Compute next minor version: v0.1.0 → v0.2.0, v1.5.0 → v1.6.0.
LATEST_BASENAME=$(basename "$LATEST_MAP" .toml | sed 's/^v//')
IFS='.' read -r MAJ MIN PATCH <<<"$LATEST_BASENAME"
FUTURE_VER="${MAJ}.$((MIN + 1)).${PATCH}"
FUTURE_MAP="$MAPS_DIR/v${FUTURE_VER}.toml"

cleanup() {
  rm -f "$FUTURE_MAP"
}
trap cleanup EXIT

# Copy the latest committed map verbatim — it's append-only, so a "future"
# release must contain every prior entry. We don't add new shrunk classes
# here because we only need the firmware to reject this PAPK.
cp "$LATEST_MAP" "$FUTURE_MAP"

echo "==> Synthetic future map: $FUTURE_MAP (copied from $(basename "$LATEST_MAP"))"

# Build the app PAPK with PICODROID_SHRINK=1; build-apk.sh's print-version
# now resolves to FUTURE_VER (highest map ≤ current Cargo.toml version
# fails — but we want it to PICK UP the future map, so temporarily bump
# the resolver lookup by exporting PICODROID_FORCE_MAP_VERSION).
#
# class-shrink doesn't honor an override env yet, so easiest path is to
# have the Cargo.toml lookup find this future file because it sits ≤ the
# package version. That requires the package version to be ≥ FUTURE_VER,
# which it isn't. So instead we run build-apk.sh with the map file at the
# expected location AND override the version that gets stamped into the
# manifest by passing an env hint to build-apk.sh.
#
# Simpler: directly call class-shrink and papk-pack with the future map.

HOST_TARGET="$(rustc -vV | awk '/^host:/{print $2}')"

# Compile framework + app into temp classes dirs.
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"; cleanup' EXIT

FRAMEWORK_CLASSES_DIR="$TMP/framework-classes"
APP_CLASSES_DIR="$TMP/app-classes-${APP}"
SHRUNK_DIR="$TMP/shrunk-${APP}"
mkdir -p "$FRAMEWORK_CLASSES_DIR" "$APP_CLASSES_DIR" "$SHRUNK_DIR"

# Framework
mapfile -t FRAMEWORK_JAVA_FILES < <(find "$REPO_ROOT/sdk/java" -name '*.java')
javac --release 8 -Xlint:-options -d "$FRAMEWORK_CLASSES_DIR" "${FRAMEWORK_JAVA_FILES[@]}"

# App
mapfile -t APP_JAVA_FILES < <(find "$REPO_ROOT/examples/$APP" -name '*.java')
javac --release 8 -Xlint:-options -cp "$FRAMEWORK_CLASSES_DIR" \
  -d "$APP_CLASSES_DIR" "${APP_JAVA_FILES[@]}"

# Apply the future map to the app classes.
cargo run --quiet --target "$HOST_TARGET" \
  --manifest-path "$REPO_ROOT/tools/class-shrink/Cargo.toml" -- \
  shrink-dir --in "$APP_CLASSES_DIR" --out "$SHRUNK_DIR" --map "$FUTURE_MAP"

# Read app manifest for entry-point classes.
mapfile -t MANIFEST_ATTRS < <(python3 - "$REPO_ROOT/examples/$APP/PicodroidManifest.xml" <<'EOF'
import sys, xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
app = root.find("application")
print(app.get("main-class", ""))
print(app.get("activity", ""))
print(app.get("application", ""))
print(root.get("version", "1.0"))
EOF
)
MAIN_CLASS="${MANIFEST_ATTRS[0]}"
ACTIVITY="${MANIFEST_ATTRS[1]}"
APPLICATION="${MANIFEST_ATTRS[2]}"
VERSION="${MANIFEST_ATTRS[3]}"

PAPK_ARGS=()
[[ -n "$MAIN_CLASS" ]] && PAPK_ARGS+=(--main-class "$MAIN_CLASS")
[[ -n "$ACTIVITY" ]] && PAPK_ARGS+=(--activity "$ACTIVITY")
[[ -n "$APPLICATION" ]] && PAPK_ARGS+=(--application "$APPLICATION")

mkdir -p "$(dirname "$OUTPUT")"

cargo run --quiet --target "$HOST_TARGET" \
  --manifest-path "$REPO_ROOT/tools/papk-pack/Cargo.toml" -- \
  "${PAPK_ARGS[@]}" \
  --package-name "$APP" \
  --version "$VERSION" \
  --framework-map-version "$FUTURE_VER" \
  --classes-dir "$SHRUNK_DIR" \
  --output "$OUTPUT"

echo "==> Wrote $OUTPUT (framework-map-version=$FUTURE_VER)"
