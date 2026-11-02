#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${DIST_DIR:-$ROOT_DIR/dist}"
TARGET_DIR="$ROOT_DIR/target/release"
ARCHIVE_PATH="$DIST_DIR/gpdfview-linux-x86_64.tar.gz"

mkdir -p "$DIST_DIR"
cd "$ROOT_DIR"

cargo build --release --bin gpdfview

cp "$TARGET_DIR/gpdfview" "$DIST_DIR/gpdfview"
cp "$ROOT_DIR/README.md" "$DIST_DIR/README.md"
cp "$ROOT_DIR/LICENSE" "$DIST_DIR/LICENSE"

tar -czf "$ARCHIVE_PATH" -C "$DIST_DIR" gpdfview README.md LICENSE

echo "Release bundle created at $ARCHIVE_PATH"
