# gpdfview

**A fast, native PDF viewer for Linux, written in Rust with GTK4 and libadwaita.**

[![Language](https://img.shields.io/badge/language-Rust-orange)](https://www.rust-lang.org/)
[![GUI](https://img.shields.io/badge/UI-GTK4-blue)](https://www.gtk.org/)
[![PDF engine](https://img.shields.io/badge/PDF-Poppler-green)](https://poppler.freedesktop.org/)
[![License](https://img.shields.io/badge/license-MIT-lightgrey)](LICENSE)

`gpdfview` is a Linux PDF viewer inspired by the workflow and practical feature set of [qpdfview](https://launchpad.net/qpdfview). It reimplements the application layer in Rust and uses GTK4/libadwaita for its desktop interface and Poppler for PDF processing.

The project is focused on three requirements: **fast access to the first page, responsive navigation through long documents, and dependable printing**. It uses on-demand page rendering, a dedicated rendering worker, bounded caches and a page-by-page printing fallback instead of rasterizing an entire document in memory.

> **Project status: active alpha.** This repository contains a working application with multi-document tabs, several page layouts, document-wide text search, thumbnails, outlines, session persistence and CUPS printing. It is not yet a complete qpdfview replacement. The feature list and limitations below describe the implementation in this repository snapshot; they are not a claim of release-grade stability or complete feature parity.

## Contents

- [Project goals](#project-goals)
- [Features](#features)
- [Architecture](#architecture)
- [Performance and memory strategy](#performance-and-memory-strategy)
- [Printing](#printing)
- [Known limitations and next steps](#known-limitations-and-next-steps)
- [Technology stack](#technology-stack)
- [Requirements](#requirements)
- [Build and run](#build-and-run)
- [Tests, benchmarks and packaging](#tests-benchmarks-and-packaging)
- [Project structure](#project-structure)
- [Application data](#application-data)
- [License](#license)
- [Contributing](#contributing)

## Project goals

### Preserve a practical PDF workflow

The intention is to retain the useful parts of qpdfview's workflow: multiple documents, tab-based navigation, a compact toolbar, a sidebar, search, page layouts, keyboard shortcuts and printing. The interface is adapted to GTK4 rather than attempting to reproduce Qt widgets one-for-one.

### Keep the interface responsive

Opening a document should not require rendering every page. The current page is requested first, rendered outside the GTK main thread and presented when ready. Adjacent pages can be prefetched to improve subsequent navigation.

### Use established PDF technology

The application does not implement a PDF parser. Poppler handles PDF loading, page rendering, text search, metadata and outline extraction. Rust coordinates the application state, workers, caches and GTK integration.

### Measure performance instead of assuming it

Rust alone does not guarantee a faster viewer. Performance comparisons must use the same PDFs, machine, display session and comparable measurement points. The existing benchmark artifacts are preliminary; see [Tests, benchmarks and packaging](#tests-benchmarks-and-packaging).

## Features

The following table reflects the source code in this repository, not the older planning checklists that may still be present in some auxiliary documents.

| Area | Current implementation | Notes |
|---|---|---|
| PDF documents | Open local PDF files | Non-local URIs and other document formats are not supported by the current application flow. |
| Tabs | Multiple document tabs, close buttons and restore-last-closed-tab action | Each tab has its own viewer state and worker pair. |
| Navigation | Previous/next page and direct page selection | Page numbering shown to the user starts at 1. |
| Zoom | Zoom in/out, fit width and fit page | Zoom step is configurable. |
| Page layouts | Single page, continuous and two-page | Layout choice can be saved as the default. |
| Split view | Horizontal or vertical split, with optional synchronization | This is a separate viewing mode from the two-page layout. |
| Rotation | Rotate pages left or right | Rotation is persisted per document. |
| Text search | Search across the current document, highlight matches and move next/previous | Results include page references and can be selected from the sidebar. |
| Sidebar | Page thumbnails, PDF outline, app bookmarks, search results, recent files and favorites | Thumbnails are shown around the current page and use a separate bounded cache. |
| Bookmarks | Navigate PDF outline entries and create/remove application page bookmarks | Application bookmarks are stored separately from the PDF's embedded outline. |
| Document information | Display basic PDF metadata and page count | Metadata availability depends on the source PDF. |
| Session | Restore the last document, last directory, window geometry/state and per-document view settings | Per-document state includes page, zoom, rotation and layout. |
| Preferences | Default layout, split synchronization, zoom increment and a set of configurable shortcuts | This is a focused preferences dialog, not yet a full qpdfview-equivalent settings system. |
| Display modes | Fullscreen and presentation mode | Presentation mode uses the current document viewer. |
| Printing | CUPS submission through `lp`, with page range, copies, orientation and scale | Direct PDF submission is attempted first; a Poppler/Cairo PDF fallback is used if it fails. |

## Architecture

The application is divided into focused Rust modules. GTK widgets and viewer state are coordinated by the application layer, while Poppler and Cairo handle document operations and rendering.

```text
                      GTK4 / libadwaita
                              |
                    Application and Window
                              |
             +----------------+----------------+
             |                |                |
        Viewer state     Document tabs     UI commands
             |                |                |
             +----------------+----------------+
                              |
                 Rendering and navigation
                              |
                    PDF rendering worker
                              |
                         Poppler
                              |
                         Cairo

          Separate worker: printing -> CUPS (`lp`)
```

### Rendering pipeline

1. A navigation, zoom or rotation action creates a render request for the selected document and page.
2. The rendering worker loads or reuses the corresponding Poppler document and renders the requested page.
3. The result is returned to the GTK application through a message channel.
4. The UI checks the request generation and document state before presenting the surface. Outdated results are discarded.
5. The rendered surface is inserted into the LRU page cache. Adjacent pages may be queued for prefetching.

The worker keeps the active Poppler document while processing requests for the same file. Poppler access is guarded by a shared lock to avoid concurrent access through the current binding strategy.

### Main components

| Module | Responsibility |
|---|---|
| `app.rs` | Application lifecycle, viewer controller, document state and print dialog flow |
| `window.rs` | Main window, toolbar, menus, tab container, drawing area and event wiring |
| `engine.rs` | Poppler document access, page rendering, thumbnails, search, outline and document metadata |
| `rendering.rs` | Render request coordination, result handling and adjacent-page prefetch |
| `cache.rs` | Byte-accounted LRU cache used for rendered pages and thumbnails |
| `navigation.rs` | Page navigation, zoom, fit modes, rotation and presentation behavior |
| `viewer_state.rs` | Viewer state, layout modes, split-view state and search occurrences |
| `document_tabs.rs` | Tab bookkeeping and recently closed document paths |
| `document_actions.rs` | File selection and opening documents in tabs |
| `sidebar.rs` | Thumbnails, PDF outline, application bookmarks, search results, recents and favorites |
| `session.rs` | Preferences, recent files, favorites, bookmarks, window state and per-document persistence |
| `printing.rs` | Print worker, CUPS submission and generated-PDF fallback |
| `dialogs.rs` | Preferences and document-properties dialogs |
| `keyboard.rs` | Shortcut matching and action resolution |
| `ui_commands.rs` | Shared command dispatch for menu and toolbar actions |
| `layout.rs` | Page geometry and Cairo search-highlight helpers |
| `poppler_lock.rs` | Shared synchronization for Poppler access |

## Performance and memory strategy

Large-document handling is a primary design constraint.

### On-demand rendering

The viewer requests the page needed for the current interaction. It does not intentionally render every page when a PDF is opened.

### Background page rendering

Main-page rendering runs in a dedicated worker named `gpdfview-renderer`. The GTK main thread receives completed results and updates the interface. A generation identifier prevents a late result from replacing a newer page after rapid navigation.

### Adjacent-page prefetch

The renderer can queue the page before and after the current page. A new primary request clears obsolete prefetch work so navigation remains the priority.

### Bounded caches

The current source defines these cache budgets:

| Cache | Budget | Purpose |
|---|---:|---|
| Main rendered-page cache | 128 MiB | Reuse pages during navigation and zoom/layout operations |
| Sidebar thumbnail cache | 24 MiB | Keep small page previews without retaining unbounded image data |

Both use an LRU eviction policy with byte accounting. A rendered page larger than the cache budget is not inserted into that cache.

### Poppler synchronization

Poppler operations are protected by a shared mutex. This favors safe, serialized access with the current binding and worker design. It is not a claim that all PDF operations currently execute concurrently.

### Current performance caveats

The main page-render path is asynchronous, but not every PDF operation has been moved off the UI thread. In particular, document-wide text search and some sidebar population paths (including thumbnail and outline requests) still perform work synchronously. These paths should be profiled with very large or complex PDFs and are candidates for further worker-based execution.

The application must continue to avoid whole-document rasterization. Memory use should remain bounded by the visible workload, caches and current rendering/printing operation.

## Printing

Printing is handled by a separate worker so the GTK main loop is not responsible for submitting or rendering the print job.

The current sequence is:

1. Validate that the source path points to a local file.
2. Attempt to submit the original PDF to the default CUPS printer using the `lp` command.
3. Pass the selected page range, number of copies, orientation and scale to CUPS.
4. If direct PDF submission fails, render the requested pages sequentially with Poppler into a temporary Cairo PDF surface.
5. Submit that generated PDF to CUPS and remove the temporary file when the operation ends.

The fallback processes pages one at a time; it does not create a full-document bitmap in memory. A single print job is processed at a time by the current print service.

**Printing requirements:** CUPS must be installed and configured, and the `lp` command must be available in `PATH`. The current UI targets the default printer; printer selection and broader printer-specific options are not yet implemented.

## Known limitations and next steps

The project is functional, but several areas need additional engineering and validation before it can be considered a mature replacement for qpdfview.

### Current limitations

- Only local PDF files are supported. DjVu, PostScript and other formats are outside the current implementation.
- PDF annotations and interactive form editing are not implemented.
- Search, thumbnail generation and outline loading need further profiling and asynchronous handling for very large documents.
- The sidebar is a combined panel, not a set of independently dockable qpdfview-style panels.
- The preferences dialog covers core options only; it does not reproduce qpdfview's full configuration surface.
- Printing uses the default CUPS destination and does not yet expose printer selection or every backend-specific setting.
- Session restoration covers the last document and per-document view state; full restoration of an arbitrary multi-tab workspace still needs validation.
- Full comparative benchmarks for first visible page, navigation latency, peak memory and long-document printing have not yet been established.

### Recommended engineering priorities

1. Move full-document search, thumbnail generation and outline extraction to cancellable background tasks.
2. Replace periodic sidebar refresh work with more targeted state-change notifications where possible.
3. Validate per-tab session restoration and tab lifecycle under stress.
4. Expand print validation across CUPS backends and printers, including fallback behavior and page-range edge cases.
5. Establish reproducible qpdfview-versus-gpdfview measurements using identical PDFs and a graphical desktop session.
6. Continue closing functional parity gaps such as annotations, forms and more complete preferences.

## Technology stack

| Technology | Role |
|---|---|
| Rust 2021 | Application logic, state and worker coordination |
| GTK4 | Linux graphical interface |
| libadwaita | GTK desktop application components |
| Poppler / `poppler-rs` | PDF loading, rendering, text search and document structure |
| Cairo | Page surfaces, thumbnails and PDF printing fallback |
| GLib / GIO | Main-loop integration and local file handling |
| CUPS (`lp`) | Print submission |

The Rust dependency versions are defined in `Cargo.toml` and pinned by `Cargo.lock`. The GUI is currently built with GTK4 features `v4_10` and `v4_12`; the native development packages must be compatible with those features.

## Requirements

A Linux desktop environment with GTK4 support is required to run the graphical application. Development packages are needed to compile the Rust bindings and their native dependencies.

### Arch Linux / BigLinux

```bash
sudo pacman -S --needed \
  base-devel \
  rust \
  cargo \
  gtk4 \
  libadwaita \
  poppler-glib \
  cairo \
  cups
```

Package names can differ on other distributions. Install the equivalent GTK4, libadwaita, Poppler GLib, Cairo and CUPS development packages before building.

## Build and run

Clone the repository and enter its directory:

```bash
git clone https://github.com/eltonfabricio10/gpdfview.git
cd gpdfview
```

Build the application:

```bash
cargo build --locked --release --bin gpdfview
```

Run without opening a document:

```bash
cargo run --bin gpdfview
```

Open a local PDF:

```bash
cargo run --bin gpdfview -- /path/to/document.pdf
```

The application also accepts PDF files through the desktop application's file-open integration.

## Tests, benchmarks and packaging

### Unit tests

Run the Rust test suite:

```bash
cargo test --locked
```

Check formatting and linting:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

The tests are colocated with the modules and cover areas such as rendering helpers, PDF engine behavior, cache eviction, viewer state, session persistence, tab bookkeeping and print fallback generation.

### Benchmark scripts

The repository includes:

- `measure_qpdfview.sh` — helper for recording a qpdfview baseline.
- `scripts/benchmark.sh` — launches gpdfview against the PDF fixtures and records a basic startup/smoke-test report.
- `BENCHMARK_REFERENCE.md` — reference hardware, software and test-document information.
- `measurements/` — recorded benchmark artifacts.

Run the gpdfview smoke benchmark from a graphical session:

```bash
./scripts/benchmark.sh
```

**Important measurement limitation:** the current script waits approximately one second and records whether the launched process remains alive. It does not instrument the moment the first page becomes visible, and its current RSS sampling is not a reliable measurement of the application's complete memory use. Its output must not be compared directly with qpdfview's recorded first-page timings. A proper benchmark should instrument the application process and the first rendered page in an active X11/Wayland session.

The stored qpdfview baseline records roughly 329–333 ms and 113–117 MB RSS for the small reference PDFs. The current gpdfview smoke-test rows report a process-start status around one second, not an equivalent first-page measurement. These figures do **not** establish that either application is faster or more memory-efficient.

### Linux release archive

Build and package the application:

```bash
./scripts/package_release.sh
```

The script creates:

```text
dist/gpdfview-linux-x86_64.tar.gz
```

The archive contains the release binary, `README.md` and `LICENSE`. The script currently targets the host architecture and is not a cross-compilation or AppImage/Flatpak packaging system.

## Project structure

```text
gpdfview/
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── BENCHMARK_REFERENCE.md
├── PARITY_MATRIX.md
├── NEW_FEATURE_ROADMAP.md
├── QPDFVIEW_PARITY_BLOCK_PLAN.md
├── scripts/
│   ├── benchmark.sh
│   └── package_release.sh
├── measurements/
├── tests/
│   └── fixtures/
└── src/
    ├── main.rs
    ├── app.rs
    ├── window.rs
    ├── engine.rs
    ├── rendering.rs
    ├── cache.rs
    ├── navigation.rs
    ├── viewer_state.rs
    ├── document_tabs.rs
    ├── document_actions.rs
    ├── sidebar.rs
    ├── session.rs
    ├── printing.rs
    ├── dialogs.rs
    ├── keyboard.rs
    ├── ui_commands.rs
    ├── layout.rs
    ├── ui_helpers.rs
    ├── poppler_lock.rs
    └── bin/
        └── test_poppler.rs
```

## Application data

User preferences and session data are stored locally under:

```text
~/.local/share/gpdfview/
```

This directory is used for settings, recent documents, favorites, page bookmarks, per-document view state, last-directory information and window/session state. The current application does not need a network service to open or view local PDF files.

## License

The repository's top-level [`LICENSE`](LICENSE) file declares the project under the **MIT License**. Third-party libraries retain their own licenses and notices; review the relevant dependency terms when preparing redistributed binaries or source bundles.

qpdfview is an inspiration for the application's workflow and feature goals. The project should preserve appropriate attribution and must not copy source code or assets without satisfying their applicable license terms.

## Contributing

Contributions are welcome, particularly in these areas:

- Rust, GTK4 and libadwaita application development.
- Poppler integration and safe FFI boundaries.
- Rendering performance, memory profiling and cancellation.
- Search, thumbnails, outlines and large-document navigation.
- CUPS printing and print fallback validation.
- Automated tests, accessibility and Linux packaging.

Before submitting a substantial change:

1. Explain the user-visible behavior and affected modules.
2. Keep GTK widget operations on the main thread.
3. Keep PDF processing behind the engine boundary.
4. Avoid whole-document rasterization and unbounded image caches.
5. Add tests for state transitions and failure cases.
6. Run `cargo fmt`, `cargo test` and `cargo clippy` where the development environment permits.
7. Update this README and the parity/roadmap documents when implementation status changes.

## Project vision

**gpdfview aims to combine qpdfview's practical document workflow with a modern Rust and GTK4 application architecture.** Its success should be measured by the quality of its rendering, responsiveness, memory behavior, printing and functional completeness—not by the language or toolkit alone.
