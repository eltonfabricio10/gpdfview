#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BIN="${APP_BIN:-$ROOT_DIR/target/debug/gpdfview}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/measurements}"
mkdir -p "$OUTPUT_DIR"

if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  echo "GUI benchmark skipped: no active DISPLAY/WAYLAND session detected."
  exit 0
fi

if [[ $# -gt 0 ]]; then
  pdfs=("$@")
else
  mapfile -t pdfs < <(find "$ROOT_DIR/tests/fixtures" -maxdepth 1 -type f -name "*.pdf" | sort)
fi

if [[ ${#pdfs[@]} -eq 0 ]]; then
  echo "No PDF fixtures were found." >&2
  exit 1
fi

if [[ ! -x "$APP_BIN" ]]; then
  echo "Building gpdfview before benchmarking..."
  cargo build --quiet --bin gpdfview --manifest-path "$ROOT_DIR/Cargo.toml"
fi

report="$OUTPUT_DIR/gpdfview_benchmark.tsv"
: > "$report"

for pdf in "${pdfs[@]}"; do
  if [[ ! -f "$pdf" ]]; then
    echo "Skipping missing file: $pdf" >&2
    continue
  fi

  filename="$(basename "$pdf")"
  start_ns=$(date +%s%N)
  timeout 12s "$APP_BIN" "$pdf" >/tmp/gpdfview-benchmark.log 2>&1 &
  pid=$!

  sleep 1
  if kill -0 "$pid" 2>/dev/null; then
    rss_kb=$(awk '/VmRSS/ {print $2}' "/proc/$pid/status" 2>/dev/null || echo 0)
    status="started"
  else
    rss_kb=0
    status="failed"
  fi

  end_ns=$(date +%s%N)
  elapsed_ms=$(((end_ns - start_ns) / 1000000))

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true

  printf '%s\t%s\t%s\t%s\n' "$filename" "$elapsed_ms" "$rss_kb" "$status" >> "$report"
  echo "Recorded $filename: ${elapsed_ms} ms, RSS ${rss_kb} KB, status=${status}"
done

echo "Benchmark report written to $report"
