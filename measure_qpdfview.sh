#!/bin/bash

pdf="$1"
pdf_name=$(basename "$pdf")
echo "=== Measuring $pdf_name ==="

for iteration in 1 2 3; do
  echo "Iteration $iteration:"
  
  # Registra timestamp inicial
  start=$(date +%s%N)
  
  # Inicia qpdfview em background
  qpdfview "$pdf" > /dev/null 2>&1 &
  PID=$!
  
  # Espera um pouco para certeza de que está aberto
  sleep 0.3
  
  # Captura memória (RSS em KB)
  if [[ -f /proc/$PID/status ]]; then
    mem_kb=$(grep "^VmRSS" /proc/$PID/status | awk '{print $2}')
    echo "  Memory (RSS): ${mem_kb} KB"
  fi
  
  # Mata processo
  kill $PID 2>/dev/null
  wait $PID 2>/dev/null
  
  # Timestamp final
  end=$(date +%s%N)
  elapsed_ms=$(( (end - start) / 1000000 ))
  echo "  Wall time: ${elapsed_ms} ms"
  
  # Pausa entre iterações
  sleep 0.2
done
