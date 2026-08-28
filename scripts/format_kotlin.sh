#!/usr/bin/env bash
# Kotlin formatting gate (ktfmt, kotlinlang style) — the Kotlin twin of
# format_java.sh. Covers examples/, kotlin-shim/ and tools/kotlin-survey/;
# buildSrc is deliberately out of scope, as it is for Java.
set -e

VENDOR_DIR="$(cd "$(dirname "$0")/.." && pwd)/vendor"
JAR_VERSION="0.64"
JAR_NAME="ktfmt-${JAR_VERSION}-with-dependencies.jar"
JAR_PATH="${VENDOR_DIR}/${JAR_NAME}"
JAR_URL="https://repo1.maven.org/maven2/com/facebook/ktfmt/${JAR_VERSION}/${JAR_NAME}"
# Update this when bumping JAR_VERSION: shasum -a 256 <downloaded-jar>
JAR_SHA256="5b3d5286fd2defcc7dc8e28c21ddf156cc6b2d8682bdcd929ce4333e7a6201f2"

verify_jar() {
  echo "${JAR_SHA256}  ${JAR_PATH}" | shasum -a 256 --check --quiet
}

if [[ ! -f "$JAR_PATH" ]]; then
  echo "==> Downloading ktfmt ${JAR_VERSION}..."
  mkdir -p "$VENDOR_DIR"
  curl -fsSL "$JAR_URL" -o "$JAR_PATH"
  echo "==> Downloaded to ${JAR_PATH}"
fi

if ! verify_jar; then
  echo "ERROR: SHA256 mismatch for ${JAR_NAME}. Delete ${JAR_PATH} and re-run." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

MODE="${1:-check}"

# Prune build/ dirs (generated sources are not hand-maintained). Collected
# into an array so an empty set is a no-op on both GNU and BSD userlands.
FILES=()
while IFS= read -r -d '' f; do FILES+=("$f"); done < <(
  find "$REPO_ROOT/examples" "$REPO_ROOT/kotlin-shim" "$REPO_ROOT/tools/kotlin-survey" \
    -type d -name build -prune -o -name '*.kt' -print0
)

if [[ "$MODE" == "check" ]]; then
  echo "==> Checking Kotlin formatting..."
  if [[ ${#FILES[@]} -gt 0 ]] && ! java -jar "$JAR_PATH" --kotlinlang-style --dry-run --set-exit-if-changed "${FILES[@]}"; then
    echo ""
    echo "ERROR: Kotlin formatting check failed."
    echo "       Run './scripts/format_kotlin.sh format' to fix, then re-stage your changes."
    exit 1
  fi
  echo "==> Kotlin formatting OK (${#FILES[@]} files)."
elif [[ "$MODE" == "format" ]]; then
  echo "==> Formatting Kotlin files..."
  if [[ ${#FILES[@]} -gt 0 ]]; then
    java -jar "$JAR_PATH" --kotlinlang-style "${FILES[@]}"
  fi
  echo "==> Done."
else
  echo "Usage: $0 [check|format]"
  echo "  check  (default) Fail if any file is not formatted."
  echo "  format           Reformat files in-place."
  exit 1
fi
