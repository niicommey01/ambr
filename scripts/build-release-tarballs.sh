#!/usr/bin/env bash
# Build release binaries for linux amd64 and arm64 and pack them as .tar.gz.
# Usage: ./scripts/build-release-tarballs.sh
# Requires: rustup
# For arm64 cross-compile on x86_64 Linux: install gcc-aarch64-linux-gnu and
# optionally create .cargo/config.toml with [target.aarch64-unknown-linux-gnu] linker = "aarch64-linux-gnu-gcc"

set -e
cd "$(dirname "$0")/.."
NAME=ambr
VERSION="${VERSION:-$(cargo pkgid -p ambr 2>/dev/null | sed -n 's/.*#\(.*\)/\1/p' || echo '0.1.0')}"
OUT_DIR=dist
mkdir -p "$OUT_DIR"

# Targets: linux x86_64 (amd64) and linux aarch64 (arm64)
TARGET_AMD64=x86_64-unknown-linux-gnu
TARGET_ARM64=aarch64-unknown-linux-gnu

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
    $TARGET_AMD64) suffix=linux_amd64 ;;
    $TARGET_ARM64) suffix=linux_arm64 ;;
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
