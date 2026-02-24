#!/usr/bin/env bash
# Build macOS releases (Intel and Apple Silicon). Must be run on a Mac.
# Usage: ./scripts/build-darwin.sh
# Produces: dist/ambr-darwin_amd64.tar.gz, dist/ambr-darwin_arm64.tar.gz
# Cross-compiling to macOS from Linux is not supported (requires macOS SDK).

set -e
if [ "$(uname -s)" != "Darwin" ]; then
  echo "Error: This script must be run on macOS (Darwin)."
  echo "Cross-compiling to macOS from Linux is not supported."
  echo "Options: run this script on a Mac, or use CI (e.g. GitHub Actions with runs-on: macos-latest)."
  exit 1
fi

cd "$(dirname "$0")/.."
NAME=ambr
OUT_DIR=dist
mkdir -p "$OUT_DIR"

TARGET_AMD64=x86_64-apple-darwin
TARGET_ARM64=aarch64-apple-darwin

echo "=== Adding targets (if missing) ==="
rustup target add $TARGET_AMD64
rustup target add $TARGET_ARM64

echo "=== Building $TARGET_AMD64 ==="
cargo build --release --target $TARGET_AMD64
echo "=== Building $TARGET_ARM64 ==="
cargo build --release --target $TARGET_ARM64

echo "=== Creating tarballs ==="
for target in $TARGET_AMD64 $TARGET_ARM64; do
  case $target in
    $TARGET_AMD64) suffix=darwin_amd64 ;;
    $TARGET_ARM64) suffix=darwin_arm64 ;;
    *) suffix=$target ;;
  esac
  dir="$OUT_DIR/${NAME}-${suffix}"
  rm -rf "$dir"
  mkdir -p "$dir"
  cp "target/$target/release/$NAME" "$dir/"
  cp README.md LICENSE "$dir/" 2>/dev/null || true
  tar -czvf "$OUT_DIR/${NAME}-${suffix}.tar.gz" -C "$OUT_DIR" "${NAME}-${suffix}"
  rm -rf "$dir"
  echo "  -> $OUT_DIR/${NAME}-${suffix}.tar.gz"
done

echo "=== Done ==="
ls -la "$OUT_DIR"/*.tar.gz
