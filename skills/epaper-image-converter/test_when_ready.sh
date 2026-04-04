#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

BINARY="./scripts/epaper_converter"
INPUT="./assets/test_gradient.jpg"
OUT_BIN="/tmp/epaper_test.bin"
OUT_BMP="/tmp/epaper_test.bmp"

if [ ! -x "$BINARY" ]; then
  echo "Binary not found or not executable: $BINARY" >&2
  exit 1
fi

if [ ! -f "$INPUT" ]; then
  echo "Missing test asset: $INPUT" >&2
  exit 1
fi

echo "== binary =="
ls -lh "$BINARY"

echo
echo "== convert(bin, auto) =="
"$BINARY" convert "$INPUT" "$OUT_BIN" -f bin -d auto --resize-mode contain -b
ls -lh "$OUT_BIN"

echo
echo "== convert(bmp, floyd) =="
"$BINARY" convert "$INPUT" "$OUT_BMP" -f bmp -d floyd --resize-mode contain -b
ls -lh "$OUT_BMP"

echo
echo "== check(converted bmp) =="
"$BINARY" check "$OUT_BMP" --verbose

echo
echo "== benchmark =="
"$BINARY" benchmark "$INPUT"

echo
echo "All checks passed."
