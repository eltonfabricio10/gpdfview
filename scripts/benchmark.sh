#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BIN="${APP_BIN:-$ROOT_DIR/target/debug/gpdfview}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/measurements}"
SAMPLE_TIMEOUT_MS="${SAMPLE_TIMEOUT_MS:-10000}"
mkdir -p "$OUTPUT_DIR"

if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  echo "Benchmark GUI ignorado: DISPLAY/WAYLAND_DISPLAY não está ativo."
  exit 0
fi

if [[ $# -gt 0 ]]; then
  pdfs=("$@")
else
  mapfile -t pdfs < <(find "$ROOT_DIR/tests/fixtures" -maxdepth 1 -type f -name "*.pdf" | sort)
fi

if [[ ${#pdfs[@]} -eq 0 ]]; then
  echo "Nenhum PDF foi encontrado." >&2
  exit 1
fi

if [[ ! -x "$APP_BIN" ]]; then
  cargo build --quiet --bin gpdfview --manifest-path "$ROOT_DIR/Cargo.toml"
fi

report="$OUTPUT_DIR/gpdfview_benchmark.tsv"
printf 'file\twindow_ms\tprocess_sample_ms\trss_kb\tstatus\n' > "$report"

for pdf in "${pdfs[@]}"; do
  if [[ ! -f "$pdf" ]]; then
    echo "Arquivo ausente ignorado: $pdf" >&2
    continue
  fi

  start_ns=$(date +%s%N)
  "$APP_BIN" "$pdf" >"$OUTPUT_DIR/gpdfview-last.log" 2>&1 &
  app_pid=$!
  window_ms=""
  rss_kb=0
  status="failed"
  deadline_ms=$(( $(date +%s%3N) + SAMPLE_TIMEOUT_MS ))

  while [[ $(date +%s%3N) -lt $deadline_ms ]] && kill -0 "$app_pid" 2>/dev/null; do
    window_id=$(xdotool search --pid "$app_pid" --onlyvisible --name "gpdfview|$(basename "$pdf")" 2>/dev/null | head -n1 || true)
    if [[ -n "$window_id" ]]; then
      now_ns=$(date +%s%N)
      window_ms=$(((now_ns - start_ns) / 1000000))
      rss_kb=$(awk '/^VmRSS:/ {print $2}' "/proc/$app_pid/status" 2>/dev/null || echo 0)
      status="window-visible"
      break
    fi
    sleep 0.05
  done

  if [[ -z "$window_ms" ]] && kill -0 "$app_pid" 2>/dev/null; then
    rss_kb=$(awk '/^VmRSS:/ {print $2}' "/proc/$app_pid/status" 2>/dev/null || echo 0)
    status="process-live-no-window"
  fi

  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true

  filename="$(basename "$pdf")"
  sample_ns=$(date +%s%N)
  process_sample_ms=$(((sample_ns - start_ns) / 1000000))
  printf '%s\t%s\t%s\t%s\t%s\n' "$filename" "${window_ms:-timeout}" "$process_sample_ms" "$rss_kb" "$status" >> "$report"
  echo "Registrado $filename: janela=${window_ms:-timeout} ms, amostra=${process_sample_ms} ms, RSS=${rss_kb} KB, status=${status}"
done

{
  echo "timestamp=$(date --iso-8601=seconds)"
  echo "commit=$(git -C "$ROOT_DIR" rev-parse HEAD)"
  echo "kernel=$(uname -srmo)"
  echo "display=${DISPLAY:-none}"
  echo "wayland=${WAYLAND_DISPLAY:-none}"
  echo "rust=$(rustc --version)"
  echo "gtk=$(pkg-config --modversion gtk4 2>/dev/null || echo unavailable)"
  echo "poppler=$(pkg-config --modversion poppler-glib 2>/dev/null || echo unavailable)"
} > "$OUTPUT_DIR/gpdfview_environment.txt"

echo "Relatório: $report"
echo "Ambiente: $OUTPUT_DIR/gpdfview_environment.txt"
