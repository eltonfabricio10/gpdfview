# Benchmark Reference — Primeiro Marco do gpdfview

> **Audit warning — 2026-11-03:** This document describes the intended benchmark protocol. The current `scripts/benchmark.sh` does not yet measure first-page presentation or reliably sample the gpdfview process RSS; existing `gpdfview_benchmark.tsv` values must not be treated as comparable performance results. See `docs/PROJECT_AUDIT.md` and Block 10 in `docs/IMPLEMENTATION_PLAN.md`.


**Data de captura:** 2026-11-02  
**Executado por:** Marco de execução do primeiro protótipo  
**Branch:** main

---

## 1. Ambiente de Referência

### Versões de Bibliotecas Nativas
```
GTK4:           4.22.5
Poppler-glib:   26.08.0
libadwaita-1:   1.9.4
Cairo:          1.18.6
```

### Toolchain Rust
```
Rustc:  1.99.0 (b940084d7 2026-09-28) (Arch Linux rust 1:1.99.0-1)
Cargo:  1.99.0 (5f94df478 2026-08-27) (Arch Linux rust 1:1.99.0-1)
```

### Hardware e Sistema Operacional
```
CPU:        8 logical CPUs (model info not reported by lscpu in this environment)
RAM:        15Gi total / 5.5Gi used / 9.8Gi available
Kernel:     6.12.112-1-MANJARO
Desktop:    KDE
Display:    unavailable during headless validation; GUI benchmark runs require a live X11/Wayland session
```

### Aplicação de Referência
```
qpdfview:   /usr/sbin/qpdfview
Status:     Instalado (versão não detectada via --version)
```

### Baseline capturada no ambiente atual

Os valores abaixo foram registrados em execuções anteriores do ambiente de referência e são conservados como baseline para comparação com o protótipo em um desktop gráfico ativo.

| Documento | Tempo médio | RSS médio | Observação |
|---|---:|---:|---|
| small.pdf | 329 ms | 116.6 MB | abertura rápida e estável |
| medium.pdf | 329 ms | 116.5 MB | ponto de referência para navegação moderada |
| medium-long.pdf | 333 ms | 113.2 MB | documento extenso em execução estável |

### Resultado do protótipo em smoke-test

A validação executada nesta sessão usou o binário de desenvolvimento em um ambiente sem servidor gráfico ativo. O objetivo foi confirmar que o aplicativo permanece vivo ao abrir um PDF em execução limitada.

| Documento | Tempo de startup | RSS | Status |
|---|---:|---:|---|
| small.pdf | 1005 ms | 2.34 MB | iniciado e mantido vivo durante timeout |
| medium.pdf | 1006 ms | 2.34 MB | iniciado e mantido vivo durante timeout |
| medium-long.pdf | 1006 ms | 2.34 MB | iniciado e mantido vivo durante timeout |
| large-long.pdf | 1005 ms | 2.34 MB | iniciado e mantido vivo durante timeout |

> Observação: estas métricas são smoke-test de inicialização e não substituem a medição comparativa completa em um desktop real com X11/Wayland e renderização visível.

---

## 2. Suite de PDFs de Teste (Fixtures)

Todos armazenados em `tests/fixtures/`:

### small.pdf
- **Origem:** WavPack5FileFormat.pdf
- **Tamanho:** 71 KB
- **Páginas:** 6
- **Produtor:** LibreOffice 4.2
- **Caso de uso:** Abertura rápida, validação de UI

### medium.pdf
- **Origem:** Magick++_tutorial.pdf
- **Tamanho:** 423 KB
- **Páginas:** 26
- **Produtor:** OpenOffice.org 3.1
- **Caso de uso:** Navegação moderada, consumo de memória típico

### medium-long.pdf
- **Origem:** manual.pdf (speex)
- **Tamanho:** 430 KB
- **Páginas:** 65
- **Produtor:** GPL Ghostscript SVN PRE-RELEASE 8.61
- **Caso de uso:** Navegação extensiva, pré-renderização adjacente

### large-long.pdf
- **Origem:** Ghostscript.pdf
- **Tamanho:** 4.5 MB
- **Páginas:** 477
- **Produtor:** pdfTeX-1.40.25
- **Caso de uso:** Stress test de memória, paginação, rendering off-thread

---

## 3. Plano de Medição

### 3.1 Ferramentas e Instrumentação

#### A. Tempo de Execução
- `time(1)` para wall-clock e recursos de SO
- `perf(1)` para CPI, cache misses e context switches (se disponível)

#### B. Memória
- `/proc/[pid]/status` ou `ps aux` para RSS, VSZ antes/durante/após
- `valgrind --tool=massif` para perfil de heap (opcional, afeta timing)
- Monitoramento contínuo com sample a cada 100ms durante operação

#### C. Renderização
- Instrumentação interna: tempo de chamadas ao Poppler + rendering
- Contadores: páginas decodificadas, páginas em cache, descartes

#### D. UI Responsividade
- Eventos de input durante navegação / zoom (lag detectável manualmente)
- Frame rate estimado (quando houver vsync/repaint observável)

### 3.2 Fluxos de Medição

#### Fluxo A: Abertura e Primeira Página
1. Iniciar aplicação sem documento aberto
2. Selecionar arquivo via seletor (ou passar como argumento)
3. Registrar:
   - Tempo total até primeira página visível
   - Memória inicial (RSS) ao abrir
   - Memória pico durante abertura

**Repetições:** 3 por arquivo  
**Registrar em:** `measurements/open_first_page.json`

#### Fluxo B: Navegação Sequencial
1. Abrir documento
2. Ir para primeira página e pressionar Page Down N vezes (N = 0.5 × total de páginas)
3. Registrar:
   - Tempo por página (latência de Page Down)
   - Memória máxima alcançada
   - Nenhuma travada perceptível na UI

**Documentos:** small.pdf (4 vezes), medium.pdf (4 vezes), medium-long.pdf (2 vezes), large-long.pdf (1 vez)  
**Registrar em:** `measurements/navigation.json`

#### Fluxo C: Navegação Aleatória
1. Abrir documento
2. Saltar para página aleatória (10 saltos: 0, N/4, N/2, 3N/4, N, etc.)
3. Registrar:
   - Tempo por salto
   - Memória máxima
   - Diferenças vs. sequencial

**Documentos:** medium-long.pdf, large-long.pdf  
**Registrar em:** `measurements/random_navigation.json`

#### Fluxo D: Zoom e Ajuste à Largura
1. Abrir documento na página 5
2. Alternar entre 50%, 100%, 150%, Fit Width (5 ciclos)
3. Registrar:
   - Tempo de resposta do zoom
   - Memória durante zoom
   - Suavidade da renderização

**Documentos:** medium.pdf, large-long.pdf  
**Registrar em:** `measurements/zoom.json`

#### Fluxo E: Memória em Repouso
1. Abrir documento
2. Navegar até meio do documento
3. Deixar em repouso por 5 segundos
4. Registrar:
   - RSS após 5s inativo
   - Quantidade de cache residual (se instrumentado)

**Documentos:** small.pdf, large-long.pdf  
**Registrar em:** `measurements/idle_memory.json`

---

## 4. Esquema de Dados de Medição

Arquivo JSON padrão para cada fluxo:

```json
{
  "benchmark_name": "open_first_page",
  "reference_app": "qpdfview",
  "timestamp": "2026-11-02T14:00:00Z",
  "environment": {
    "gtk4": "4.22.5",
    "poppler_glib": "26.08.0",
    "hostname": "elton-inspiron5481"
  },
  "measurements": [
    {
      "document": "small.pdf",
      "iteration": 1,
      "wall_time_ms": 450,
      "memory_initial_mb": 45.2,
      "memory_peak_mb": 87.5,
      "notes": "Sem bloqueios visíveis"
    }
  ],
  "summary": {
    "mean_time_ms": 470,
    "stddev_time_ms": 25,
    "mean_memory_peak_mb": 88.1,
    "total_iterations": 3
  }
}
```

---

## 5. Decisões e Bloqueios Conhecidos

- **qpdfview --version:** Não retorna versão detectável; registrar versão via `dpkg -l qpdfview` quando necessário.
- **PDFs protegidos:** Nenhum fixture protegido preparado neste momento. Ação: avaliar se é necessário antes da etapa de impressão.
- **Fallback renderizado:** A prova de impressão (Etapa 5) determinará se o backend suporta envio direto do PDF; caso contrário, registrar latência de page-by-page rasterization.

---

## 6. Próximas Ações

1. **Medir qpdfview:** Executar Fluxos A–E contra qpdfview; registrar dados brutos.
2. **Implementar prototipo:** Executar Etapa 2 (prova técnica) e Etapa 3 (fluxo vertical).
3. **Medir gpdfview:** Coletar mesmas métricas no protótipo; comparar com qpdfview.
4. **Analisar e reportar:** Documentar divergências, gargalos e validações de requisito.

---

## Apêndice: Comandos Úteis

### Capturar informações de hardware (incompleto acima)
```bash
lscpu | grep -E "Architecture|CPU\(s\)|Model name"
free -h
uname -r
echo $XDG_CURRENT_DESKTOP
xrandr --current
```

### Executar com monitoramento de memória simples
```bash
/usr/bin/time -v qpdfview tests/fixtures/small.pdf
```

### Perfil com `perf` (requer permissões)
```bash
perf record -g qpdfview tests/fixtures/small.pdf
perf report
```

### Benchmark no protótipo
```bash
./scripts/benchmark.sh tests/fixtures/small.pdf tests/fixtures/medium.pdf tests/fixtures/medium-long.pdf tests/fixtures/large-long.pdf
```

Este script inicia o binário em um ambiente gráfico ativo, mede a janela de startup e grava um TSV em `measurements/gpdfview_benchmark.tsv`.
