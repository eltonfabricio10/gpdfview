#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${DIST_DIR:-$ROOT_DIR/dist}"
# Resolve relative destinations against the caller's working directory, not ROOT_DIR.
mkdir -p "$DIST_DIR"
DIST_DIR="$(cd "$DIST_DIR" && pwd)"
TARGET_DIR="$ROOT_DIR/target"
ARCH="$(uname -m)"
ARCHIVE_PATH="$DIST_DIR/gpdfview-linux-$ARCH.tar.gz"

cd "$ROOT_DIR"

cargo build --locked --release --bin gpdfview --target-dir "$TARGET_DIR"

cp "$TARGET_DIR/release/gpdfview" "$DIST_DIR/gpdfview"
cp "$ROOT_DIR/README.md" "$DIST_DIR/README.md"
cp "$ROOT_DIR/LICENSE" "$DIST_DIR/LICENSE"

tar -czf "$ARCHIVE_PATH" -C "$DIST_DIR" gpdfview README.md LICENSE

echo "Release bundle created at $ARCHIVE_PATH"
