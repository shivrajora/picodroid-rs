#!/usr/bin/env bash
set -e

TOOLS_DIR="$(cd "$(dirname "$0")/.." && pwd)/tools"
JAR_VERSION="1.35.0"
JAR_NAME="google-java-format-${JAR_VERSION}-all-deps.jar"
JAR_PATH="${TOOLS_DIR}/${JAR_NAME}"
JAR_URL="https://github.com/google/google-java-format/releases/download/v${JAR_VERSION}/${JAR_NAME}"

if [[ ! -f "$JAR_PATH" ]]; then
  echo "==> Downloading google-java-format ${JAR_VERSION}..."
  mkdir -p "$TOOLS_DIR"
  curl -fsSL "$JAR_URL" -o "$JAR_PATH"
  echo "==> Downloaded to ${JAR_PATH}"
fi

JAVA_DIR="$(cd "$(dirname "$0")/.." && pwd)/java"

if [[ -z "$(find "$JAVA_DIR" -name '*.java' -print -quit)" ]]; then
  echo "No Java files found."
  exit 0
fi

MODE="${1:-check}"

if [[ "$MODE" == "check" ]]; then
  echo "==> Checking Java formatting..."
  if ! find "$JAVA_DIR" -name '*.java' -print0 | xargs -0 java -jar "$JAR_PATH" --dry-run --set-exit-if-changed; then
    echo ""
    echo "ERROR: Java formatting check failed."
    echo "       Run './scripts/format_java.sh format' to fix, then re-stage your changes."
    exit 1
  fi
  echo "==> Java formatting OK."
elif [[ "$MODE" == "format" ]]; then
  echo "==> Formatting Java files..."
  find "$JAVA_DIR" -name '*.java' -print0 | xargs -0 java -jar "$JAR_PATH" --replace
  echo "==> Done."
else
  echo "Usage: $0 [check|format]"
  echo "  check  (default) Fail if any file is not formatted."
  echo "  format           Reformat files in-place."
  exit 1
fi
