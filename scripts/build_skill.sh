#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SKILL_DIR="$ROOT_DIR/skills/epaper-image-converter"
SKILL_SCRIPTS_DIR="$SKILL_DIR/scripts"

TARGET="${TARGET:-aarch64-unknown-linux-musl}"
PROFILE="${PROFILE:-release}"
BINARY_NAME="epaper_converter"
CARGO_BIN="${CARGO_BIN:-}"

if [[ -z "$CARGO_BIN" ]]; then
  if command -v cargo >/dev/null 2>&1; then
    CARGO_BIN="$(command -v cargo)"
  elif [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
  else
    echo "cargo not found. Set CARGO_BIN explicitly or install cargo into PATH." >&2
    exit 1
  fi
fi

case "$TARGET" in
  aarch64-unknown-linux-musl)
    CARGO_CMD=("$CARGO_BIN" build-linux-arm64-musl)
    ;;
  aarch64-unknown-linux-gnu)
    CARGO_CMD=("$CARGO_BIN" build-linux-arm64)
    ;;
  *)
    echo "Unsupported TARGET: $TARGET" >&2
    echo "Supported TARGET values: aarch64-unknown-linux-musl, aarch64-unknown-linux-gnu" >&2
    exit 1
    ;;
esac

if [[ "$PROFILE" != "release" ]]; then
  echo "Unsupported PROFILE: $PROFILE" >&2
  echo "Only PROFILE=release is supported by this script." >&2
  exit 1
fi

SOURCE_BINARY="$ROOT_DIR/target/$TARGET/$PROFILE/$BINARY_NAME"
DEST_BINARY="$SKILL_SCRIPTS_DIR/$BINARY_NAME"

echo "==> Building $BINARY_NAME for $TARGET"
(
  cd "$ROOT_DIR"
  "${CARGO_CMD[@]}"
)

if [[ ! -f "$SOURCE_BINARY" ]]; then
  echo "Build succeeded but binary not found: $SOURCE_BINARY" >&2
  exit 1
fi

mkdir -p "$SKILL_SCRIPTS_DIR"
cp "$SOURCE_BINARY" "$DEST_BINARY"
chmod 755 "$DEST_BINARY"

echo "==> Copied binary to skill directory"
ls -lh "$DEST_BINARY"
