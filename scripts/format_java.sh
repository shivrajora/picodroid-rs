#!/usr/bin/env bash
set -e

VENDOR_DIR="$(cd "$(dirname "$0")/.." && pwd)/vendor"
JAR_VERSION="1.35.0"
JAR_NAME="google-java-format-${JAR_VERSION}-all-deps.jar"
JAR_PATH="${VENDOR_DIR}/${JAR_NAME}"
JAR_URL="https://github.com/google/google-java-format/releases/download/v${JAR_VERSION}/${JAR_NAME}"
# Update this when bumping JAR_VERSION: shasum -a 256 <downloaded-jar>
JAR_SHA256="bfb7f9ead6cd328389bc2da53860443bc0e805dfd08cc889bfdf43b26cb2a6e8"

verify_jar() {
  echo "${JAR_SHA256}  ${JAR_PATH}" | shasum -a 256 --check --quiet
}

if [[ ! -f "$JAR_PATH" ]]; then
  echo "==> Downloading google-java-format ${JAR_VERSION}..."
  mkdir -p "$VENDOR_DIR"
  curl -fsSL "$JAR_URL" -o "$JAR_PATH"
  echo "==> Downloaded to ${JAR_PATH}"
fi

if ! verify_jar; then
  echo "ERROR: SHA256 mismatch for ${JAR_NAME}. Delete ${JAR_PATH} and re-run." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -z "$(find "$REPO_ROOT/examples" "$REPO_ROOT/sdk" -name '*.java' -print -quit 2>/dev/null)" ]]; then
  echo "No Java files found."
  exit 0
fi

MODE="${1:-check}"

if [[ "$MODE" == "check" ]]; then
  echo "==> Checking Java formatting..."
  if ! find "$REPO_ROOT/examples" "$REPO_ROOT/sdk" -name '*.java' -print0 | xargs -0 java -jar "$JAR_PATH" --dry-run --set-exit-if-changed; then
    echo ""
    echo "ERROR: Java formatting check failed."
    echo "       Run './scripts/format_java.sh format' to fix, then re-stage your changes."
    exit 1
  fi
  echo "==> Java formatting OK."
elif [[ "$MODE" == "format" ]]; then
  echo "==> Formatting Java files..."
  find "$REPO_ROOT/examples" "$REPO_ROOT/sdk" -name '*.java' -print0 | xargs -0 java -jar "$JAR_PATH" --replace
  echo "==> Done."
else
  echo "Usage: $0 [check|format]"
  echo "  check  (default) Fail if any file is not formatted."
  echo "  format           Reformat files in-place."
  exit 1
fi
