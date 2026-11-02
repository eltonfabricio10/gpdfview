#!/usr/bin/env bash
set -euo pipefail

pdf="${1:-}"
if [[ -z "$pdf" || ! -f "$pdf" ]]; then
  echo "Usage: $0 <pdf-file>" >&2
  exit 1
fi

pdf_name=$(basename "$pdf")
echo "=== Measuring $pdf_name ==="

for iteration in 1 2 3; do
  echo "Iteration $iteration:"

  start_ns=$(date +%s%N)
  qpdfview "$pdf" >/dev/null 2>&1 &
  pid=$!

  sleep 0.3

  rss_kb=0
  if [[ -f "/proc/$pid/status" ]]; then
    rss_kb=$(awk '/^VmRSS/ {print $2}' "/proc/$pid/status")
  fi
  echo "  Memory (RSS): ${rss_kb} KB"

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true

  end_ns=$(date +%s%N)
  elapsed_ms=$(((end_ns - start_ns) / 1000000))
  echo "  Wall time: ${elapsed_ms} ms"

  sleep 0.2
done
