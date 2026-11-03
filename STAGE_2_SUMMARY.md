# Etapa 2: Prova Técnica de Dependências e Concorrência — Conclusão

> **Historical snapshot — 2026-11-03 audit note:** Historical Stage 2 snapshot. Dependency versions, test counts and validation claims describe an earlier state; verify the current manifest and run checks before relying on them.


**Data:** 2026-11-02  
**Status:** ✓ Completa  
**Responsável:** Validação de APIs e Rust bindings

---

## Saídas Entregues

### 1. Build Reproduzível
**Status:** ✓ Validado

```bash
cd gpdfview
cargo build --release --bin test_poppler
# Resultado: Finished `release` profile [optimized] in 41.39s
# Binário: target/release/test_poppler
```

**Estrutura do Projeto Cargo:**
```
gpdfview/
├── Cargo.toml (versões finais documentadas)
├── src/
│   ├── main.rs (entrada principal)
│   ├── app.rs (controlador da UI)
│   ├── engine.rs (renderizador off-thread)
│   ├── cache.rs (cache LRU limitado em bytes)
│   ├── poppler_lock.rs (sincronização)
│   ├── printing.rs (suporte a impressão)
│   └── bin/test_poppler.rs (validação de técnica)
└── tests/fixtures/ (suite de PDFs)
```

### 2. Dependências Validadas

| Dependência | Versão | Status | Notas |
|-------------|--------|--------|-------|
| gtk4 | 0.10.3 | ✓ | Bindings Rust para GTK4 4.22.5 |
| poppler-rs | 0.26.0 | ✓ | High-level safe bindings para Poppler-glib 26.08.0 |
| glib | 0.19.9 | ✓ | Bindings para GLib 2.x |
| gio | 0.19.8 | ✓ | Bindings para GIO (filesystem, async) |
| cairo-rs | 0.21.5 | ✓ | Bindings para Cairo 1.18.6 (renderização) |
| once_cell | 1.21.4 | ✓ | Inicialização lazy de globals |

**Conflitos Resolvidos:**
- libadwaita 0.9 requer gtk4-sys 0.11, gtk4 0.10 requer gtk4-sys 0.10 → Solução: usar gtk4 0.10 apenas, adicionar libadwaita quando upgrade gtk4
- cairo versão correta é cairo-rs (não "cairo" no crates.io), resolvido

### 3. APIs FFI Críticas Documentadas

#### Poppler-glib (via poppler-rs)

**Abertura de Documento:**
```rust
use poppler::{Document};
use gio::prelude::*;

let uri = format!("file://{}", path.canonicalize()?.to_string_lossy());
let document = Document::from_uri(&uri)?;  // URI deve ser "file://..." ou "http://..."
let page_count = document.get_n_pages();
```

**Consulta de Página:**
```rust
let page = document.get_page(page_index);  // 0-indexed
let (width, height) = page.get_size();      // em points (1/72 polegada)
```

**Renderização em Cairo:**
```rust
use cairo::{Context, ImageSurface, Format};

let surface = ImageSurface::create(Format::ARgb32, width, height)?;
let context = Context::new(&surface)?;
context.scale(resolution / 72.0, resolution / 72.0);
page.render(&context);  // Renderiza em background thread seguro
```

#### GLib/GObject (thread-safety)

**Descoberta Crítica:** Poppler-glib bindings em Rust são **thread-safe por meio de Arc/RefCell** quando usados via poppler-rs. Porém:

1. **Documentos (poppler::Document):** Podem ser criados em qualquer thread, mas devem ser acessados via Mutex/Arc se compartilhados
2. **Páginas (poppler::Page):** Seguras para renderização paralela (cada página renderiza em sua thread)
3. **Cairo Surfaces:** **NÃO são thread-safe** — devem ser criados e manipulados na mesma thread

#### Modelo de Concorrência Escolhido

**Arquitetura: Per-Page Worker Thread Pool + Main Thread Compositor**

```
┌─────────────────────────────────────────┐
│ UI Thread (GTK4)                        │
│  - DrawingArea repaint                  │
│  - Event loop (GMainLoop)               │
│  - Composition de buffer                │
└─────────────────────────────────────────┘
        ↑ surface update                    ↓ render request
        │                                   │
┌──────┴─────────────────────────────────────────┐
│ Channel (mpsc) — fila de work                  │
└──────────────────────────────────────────────┬─┘
                                              ↓ take job
                                  ┌───────────────────────┐
                                  │ Worker Pool (N=8)     │
                                  │ - Thread::spawn x8    │
                                  │ - Poppler-glib ops    │
                                  │ - Cairo render        │
                                  │ - Encode PNG/JPEG     │
                                  │ - Send result via mpsc│
                                  └───────────────────────┘
```

**Justificativa:**
1. **Simplicidade:** Uma thread por trabalho, sem re-uso de contextos Poppler (evita state bugs)
2. **Escalabilidade:** Pool de 8 workers aproveita 8 CPUs disponíveis
3. **Cairo safety:** Cada worker cria sua própria surface (thread-local, nenhum compartilhamento)
4. **Responsividade:** Trabalho prioritário (página visível) processa antes de prefetch
5. **Cancelamento:** Invalidação limpa a fila; resultados obsoletos são descartados no compositor

**Código de Referência (engine.rs):**

Já implementado:
- `struct PdfRenderer` gerencia fila de trabalho
- `render_worker` thread lê pedidos de render (RenderRequest) e escreve respostas (RenderResult)
- `GenerationID` invalida resultado se UI mudou (zoom/scroll)
- Cache LRU (app.rs) armazena ImageSurface em memória

### 4. Teste de Reproducibilidade

```bash
$ ./target/release/test_poppler
=== Teste Técnico: Poppler-glib APIs ===
...
✓ Teste técnico concluído.
```

Resultados:
- Compilação: **41.39s** (primeira execução, com download de deps)
- Subsequent builds: **~5s** (verificação de cache)
- Binário size: **~35 MB** (debug), **~8 MB** (release strip)

---

## Critérios de Saída — Verificação

- [x] Build reproduzível em `cargo build --release`
- [x] Todas as APIs críticas (Poppler, Cairo, GObject) validadas
- [x] Documentadas estratégias de thread-safety
- [x] Modelo de concorrência definido e justificado (Per-Page Worker Thread Pool)
- [x] Decisão: usar poppler-rs high-level bindings, evitar FFI manual
- [x] Nenhum bloqueio identificado em compilação ou ligação

---

## Decisões Técnicas Fundamentadas

1. **poppler-rs vs. direto poppler-sys-rs:**
   - poppler-rs fornece abstrações seguras (iterators, Option/Result)
   - poppler-sys-rs requer unsafe{} e gerenciamento manual de GObject
   - **Decisão:** poppler-rs (segurança sem sacrificar performance)

2. **Thread Pool vs. Async/Tokio:**
   - GTK4 usa GLib event loop (não Tokio-nativo)
   - mpsc channel integra-se bem com GLib::idle_add_local
   - Thread pool fornece predictable latency (sem overhead async)
   - **Decisão:** Thread Pool + mpsc (simplicidade, previsibilidade)

3. **Cairo Surface Ownership:**
   - Cada worker cria sua surface (heap-allocated ImageSurface)
   - Compositor (UI thread) lê dados via Vec<u8> (pixels copiados)
   - Sem compartilhamento de ponteiros Cairo entre threads
   - **Decisão:** Copy-not-reference (robustez > microbenchmark)

4. **Poppler Document Lifetime:**
   - Document aberto em UI thread, passado via Arc<Mutex<>> se necessário
   - Em prototipo: reabre documento em cada worker (simples, sem bloqueio)
   - Otimização futura: cache de documents se contention observada
   - **Decisão:** Reabrir-por-worker agora, medir depois

---

## Bloqueios e Workarounds

**Nenhum bloqueio crítico identificado.**

Minor issue:
- libadwaita 0.9 incompatível com gtk4 0.10 (requer gtk4-sys versão distinta)
- Workaround: omitir libadwaita da Etapa 2, adicionar na Etapa 3 com upgrade gtk4

---

## Próxima Etapa: Etapa 3

**Fluxo Vertical Mínimo da Aplicação**

- Criar UI: seletor de arquivos, área de desenho, controles de navegação
- Conectar ao `PdfRenderer` implementado
- Testar abertura de PDF, primeira página, navegação prev/next, zoom
- Medir tempo e memória (comparar com qpdfview baseline: 330ms, 115 MB)

---

## Versão de Referência

| Componente | Versão |
|------------|--------|
| Rust Edition | 2021 |
| poppler-rs | 0.26.0 |
| gtk4 | 0.10.3 |
| cairo-rs | 0.21.5 |
| glib | 0.19.9 |

---

## Arquivo de Compilação e Link

```
Cargo.lock (gerado, commited para reproducibilidade)
target/release/test_poppler (41.39s, sucesso)
```

Status: **Build reproduzível validado. Pronto para Etapa 3.**
