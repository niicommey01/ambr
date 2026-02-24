#!/usr/bin/env bash
# Build a Windows x86_64 release from Linux (cross-compile) and pack as .zip.
# Usage: ./scripts/build-windows.sh
# Requires: rustup + full MinGW toolchain. On Debian/Ubuntu:
#   sudo apt install mingw-w64 binutils-mingw-w64-x86-64
# (.cargo/config.toml sets the linker for x86_64-pc-windows-gnu)

set -e
cd "$(dirname "$0")/.."
NAME=ambr
OUT_DIR=dist
mkdir -p "$OUT_DIR"
TARGET=x86_64-pc-windows-gnu
SUFFIX=windows_amd64

# Require MinGW dlltool (often missing with only mingw-w64)
if ! command -v x86_64-w64-mingw32-dlltool &>/dev/null; then
  echo "Error: x86_64-w64-mingw32-dlltool not found."
  echo "Install the full MinGW toolchain (Debian/Ubuntu):"
  echo "  sudo apt update"
  echo "  sudo apt install mingw-w64 binutils-mingw-w64-x86-64"
  echo "If binutils-mingw-w64-x86-64 is not found, enable universe: sudo add-apt-repository universe && sudo apt update"
  exit 1
fi

echo "=== Adding target (if missing) ==="
rustup target add $TARGET

echo "=== Building $TARGET ==="
cargo build --release --target $TARGET

echo "=== Creating zip ==="
dir="$OUT_DIR/${NAME}-${SUFFIX}"
rm -rf "$dir"
mkdir -p "$dir"
cp "target/$TARGET/release/${NAME}.exe" "$dir/"
cp README.md LICENSE "$dir/" 2>/dev/null || true
(cd "$OUT_DIR" && zip -r "${NAME}-${SUFFIX}.zip" "${NAME}-${SUFFIX}")
rm -rf "$dir"
echo "  -> $OUT_DIR/${NAME}-${SUFFIX}.zip"
ls -la "$OUT_DIR/${NAME}-${SUFFIX}.zip"
