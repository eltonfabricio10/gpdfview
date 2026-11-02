# gpdfview

### A fast, lightweight and native PDF viewer for Linux

**gpdfview** is an open-source PDF viewer project written in **Rust**, built with **GTK4** and **libadwaita**, inspired by the functionality, efficiency and workflow of [qpdfview](https://github.com/lumina-desktop/qpdfview).

The project's primary goal is to reproduce the core functionality and user experience of qpdfview while adopting a modern native GTK architecture.

Performance is a fundamental requirement. gpdfview is being designed to provide fast document opening, responsive navigation, efficient rendering and reliable printing, including when working with large PDF documents containing hundreds or thousands of pages.

> **Project status:** Early prototype. The current build opens a PDF, renders pages on a dedicated Poppler worker, supports page navigation and zoom, uses a byte-bounded page cache with adjacent-page prefetch, and submits printing through CUPS with a page-by-page Cairo fallback. This is the first milestone, not qpdfview feature parity. Unit tests cover rendering, request coalescing, memory bounds and print-PDF generation; comparative qpdfview benchmarks and physical print validation remain pending.

---

## Table of Contents

- [Project Overview](#project-overview)
- [Goals and Philosophy](#goals-and-philosophy)
- [Why gpdfview?](#why-gpdfview)
- [Core Technologies](#core-technologies)
- [Architecture](#architecture)
- [Performance Strategy](#performance-strategy)
- [PDF Rendering](#pdf-rendering)
- [Printing Architecture](#printing-architecture)
- [Planned Features](#planned-features)
- [User Interface](#user-interface)
- [Project Structure](#project-structure)
- [Development Roadmap](#development-roadmap)
- [Performance Benchmarks](#performance-benchmarks)
- [Build Requirements](#build-requirements)
- [Development Guidelines](#development-guidelines)
- [Licensing](#licensing)
- [Contributing](#contributing)

---

## Project Overview

gpdfview aims to become a native Linux PDF viewer that combines the efficiency of established PDF rendering technologies with the advantages of modern GTK applications.

Rather than creating a new PDF rendering engine, the project intends to use **Poppler** as its PDF processing backend.

Rust will handle application logic, document management, concurrency, caching and integration with the graphical interface.

GTK4 and libadwaita will provide the native desktop experience.

The application is designed around three fundamental principles:

1. **Performance first:** document opening, navigation and printing must remain fast and efficient.
2. **Functional fidelity:** preserve the useful features and workflows of qpdfview.
3. **Native integration:** provide a modern Linux desktop application without depending on Qt.

## Goals and Philosophy

### Performance is not optional

A PDF viewer should not require users to wait for an entire document to be processed before displaying its first page.

gpdfview will prioritize:

- Fast document initialization.
- Immediate rendering of the current page.
- Asynchronous page rendering.
- Intelligent prefetching.
- Memory-efficient page caching.
- Responsive navigation.
- Efficient printing of large documents.

### Faithfulness to qpdfview

The project is not intended to be a radically simplified PDF reader.

Its goal is to preserve the practical functionality of qpdfview, including its document navigation model, tabs, toolbar, sidebar, keyboard shortcuts, search capabilities and printing workflow.

The graphical interface will be adapted to GTK4 and libadwaita without unnecessarily changing familiar application behavior.

### Native Linux experience

gpdfview will be developed specifically for Linux desktop environments.

The application will use GTK4 and libadwaita components, GIO integration and native desktop services wherever appropriate.

---

## Why gpdfview?

qpdfview demonstrates that a desktop PDF viewer can remain lightweight while providing advanced document navigation and printing capabilities.

gpdfview seeks to preserve these qualities while replacing its Qt-based interface with a Rust and GTK architecture.

| Aspect | qpdfview | gpdfview |
|---|---|---|
| Programming language | C++ | Rust |
| GUI framework | Qt | GTK4 |
| Desktop integration | Qt ecosystem | GTK / libadwaita |
| PDF backend | Poppler | Poppler |
| Rendering | Poppler / Qt integration | Poppler / Cairo |
| Concurrency | Existing implementation | Rust-managed worker architecture |
| Printing | Native printing integration | CUPS / IPP integration |
| Application architecture | Existing Qt architecture | Modular Rust architecture |

The objective is not to assume that Rust automatically makes the application faster.

Performance will be measured against qpdfview using identical documents and comparable system configurations.

---

## Core Technologies

| Technology | Responsibility |
|---|---|
| Rust | Application logic, state management and concurrency |
| GTK4 | Graphical interface |
| libadwaita | Native desktop components |
| Poppler GLib | PDF interpretation and rendering |
| Cairo | Graphics and rendering surfaces |
| GLib / GIO | Event loop, asynchronous operations and desktop integration |
| CUPS / IPP | Printing infrastructure |
| SQLite | Optional persistent data storage |
| GSettings | Application preferences |

### PDF Engine: Poppler

Poppler is the primary PDF backend.

It provides established PDF processing capabilities, including:

- Document loading.
- Page rendering.
- Text extraction.
- Text searching.
- Document metadata.
- Page information.
- Document outlines.
- PDF permissions and related functionality.

The project will use Rust bindings where available and direct FFI integration when necessary.

The PDF engine must remain independent of the graphical interface.

This separation will make it possible to evolve rendering, caching and printing without coupling those operations to GTK widgets.

---

## Architecture

The application will follow a modular architecture.

```text
                 +----------------------+
                 |       GTK4 UI        |
                 |      libadwaita      |
                 +----------+-----------+
                            |
                 +----------v-----------+
                 |   Application Core   |
                 |        Rust          |
                 +----------+-----------+
                            |
           +----------------+----------------+
           |                |                |
   +-------v------+ +-------v------+ +-------v------+
   | Document     | | Viewer       | | Printing     |
   | Management   | | Controller   | | Management   |
   +-------+------+ +-------+------+ +-------+------+
           |                |                |
           +----------------+----------------+
                            |
                 +----------v-----------+
                 |      PDF Engine      |
                 |     Poppler GLib     |
                 +----------+-----------+
                            |
                 +----------v-----------+
                 |   Cairo / GIO / OS   |
                 +----------------------+
```

### Main components

**Application Core**

Responsible for application lifecycle, window management, document coordination, preferences and global state.

**Document Manager**

Responsible for opening, closing and tracking PDF documents, metadata, permissions and document-specific settings.

**PDF Engine**

Provides a unified interface to Poppler, including page access, rendering, text extraction and document information.

**Viewer Controller**

Manages page navigation, zoom, layout, scrolling, page positioning and visible page state.

**Rendering System**

Coordinates background rendering, task priorities, caching and prefetching.

**Printing System**

Handles print configuration, document submission, page ranges and rendering fallbacks.

**User Interface**

Contains the application window, toolbar, tabs, sidebar, search interface and dialogs.

---

## Performance Strategy

Performance is one of the project's most important engineering requirements.

### 1. On-demand rendering

The application must never render every page simply because a document has been opened.

Only the current page and pages required for immediate navigation should be rendered initially.

### 2. Asynchronous rendering

Page rendering must run outside the GTK main thread.

The main thread should remain responsible for UI updates and GTK object interaction.

Background workers will perform rendering and deliver results through appropriate GLib channels or main-context integration.

### 3. Intelligent page caching

The viewer will use an LRU cache with memory limits.

The cache should prioritize:

- Currently visible pages.
- Recently viewed pages.
- Adjacent pages.
- Pages likely to be needed during continuous scrolling.

Old or unused rendered surfaces should be released when memory limits are reached.

### 4. Predictive prefetching

The application should anticipate navigation.

When viewing a page, nearby pages may be rendered in advance, with priority adjusted according to navigation direction and user activity.

### 5. Task cancellation

Rendering tasks that are no longer useful should be cancelled or have their results discarded.

For example, when a user jumps from page 10 to page 800, pending work for intermediate pages should not unnecessarily consume CPU and memory.

### 6. Memory management

The application must avoid:

- Loading all page images into RAM.
- Generating unnecessary full-document previews.
- Retaining obsolete rendering surfaces.
- Running unrestricted rendering workers.
- Blocking the UI during document processing.

The cache should be configurable and measured in memory usage rather than only in the number of pages.

---

## PDF Rendering

Poppler will handle PDF interpretation and page rendering.

Cairo will be used where appropriate for graphics surfaces and integration with the GTK drawing pipeline.

The rendering system should support:

- Different zoom levels.
- High-DPI displays.
- Continuous scrolling.
- Single-page navigation.
- Two-page layouts.
- Page rotation.
- Text selection.
- Search highlighting.
- Thumbnail generation.

Rendering quality and speed must be evaluated with real-world PDF files, including documents containing large images, complex fonts, transparency and mixed page dimensions.

---

## Printing Architecture

Printing is a first-class requirement.

The project must preserve the efficient printing behavior expected from qpdfview, especially for large documents.

### Preferred printing path: native PDF submission

When supported by the selected printing backend and queue, the application should submit the original PDF rather than rasterizing every page.

This approach can preserve:

- Vector graphics.
- Embedded fonts.
- Text quality.
- Original image resolution.
- Document fidelity.

### Fallback printing path: page rendering

When direct PDF submission is unavailable or unsuitable, gpdfview should use Poppler and Cairo to process pages individually.

The implementation must avoid rendering the entire document into memory.

### Printing requirements

- Printer selection.
- Page range selection.
- Copies.
- Orientation.
- Paper size.
- Scaling.
- Margins.
- Print quality where supported.
- Progress and error reporting.

The actual printing path must be validated against the capabilities of the installed CUPS/IPP backend.

---

## Planned Features

The following list describes the intended feature set.

### Essential features — P0

- [x] Open PDF documents.
- [ ] Open multiple documents.
- [ ] Tabbed document interface.
- [x] Page navigation.
- [x] Zoom controls.
- [ ] Fit page.
- [x] Fit width.
- [ ] Continuous scrolling.
- [ ] Single-page layout.
- [ ] Two-page layout.
- [ ] Page rotation.
- [ ] Text search.
- [ ] Search result highlighting.
- [ ] Document thumbnails.
- [ ] Document outline and bookmarks.
- [x] PDF printing.
- [ ] Basic session restoration.

### Advanced features — P1

- [ ] Fullscreen mode.
- [ ] Presentation mode.
- [ ] Document properties.
- [ ] Recent documents.
- [ ] Favorites.
- [ ] Per-document preferences.
- [ ] Configurable keyboard shortcuts.
- [ ] PDF annotations.
- [ ] Interactive forms.
- [ ] Advanced navigation options.

### Additional features — P2

- [ ] SyncTeX integration.
- [ ] Additional document format backends.
- [ ] Extended desktop integration.
- [ ] Advanced customization.

Feature availability will depend on the capabilities of the selected backend and the results of implementation and compatibility testing.

The current prototype handles one local PDF at a time and prints to the default CUPS destination. The local CUPS filter chain was exercised without submitting a job; output from a physical printer and comparative qpdfview measurements are still to be verified.

---

## User Interface

The interface will retain the familiar workflow of qpdfview while adopting GTK4 and libadwaita.

The first prototype currently provides an open action, page navigation, zoom controls, fit-to-width, a central page view and a status area. Tabs, the sidebar and the remaining controls are planned for later milestones.

Long-term layout:

- Header bar with document tabs and window controls.
- Toolbar with navigation, zoom, search and printing.
- Sidebar with thumbnails, document outline and bookmarks.
- Central PDF viewing area.
- Status bar with page number, total pages and zoom level.
- Context menus and keyboard shortcuts.

The interface should prioritize usability, responsiveness and consistency with modern GTK desktop applications.

A modern appearance must not come at the expense of functionality.

---

## Project Structure

Current first-milestone source tree:

```text
gpdfview/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── engine.rs
│   ├── cache.rs
│   └── printing.rs
└── tests embedded alongside the Rust modules
```

The modules deliberately keep GTK state on the main thread, with Poppler document access isolated to its worker.

---

## Development Roadmap

Development will be divided into eight phases.

| Phase | Description | Estimated time |
|---|---|---|
| 1 | qpdfview source analysis and architectural mapping | 3–5 days |
| 2 | Rust, GTK4 and Poppler foundation | 4–7 days |
| 3 | PDF engine integration | 5–8 days |
| 4 | Rendering, cache and performance | 5–8 days |
| 5 | Navigation and reading features | 5–8 days |
| 6 | Printing implementation | 5–8 days |
| 7 | Functional parity and advanced features | 7–12 days |
| 8 | Testing, packaging and stabilization | 4–7 days |

**Estimated initial development time: 38–63 days.**

This is an initial engineering estimate, not a delivery commitment. Actual duration will depend on the source-code analysis, backend capabilities and compatibility requirements.

### First development milestone — in progress

The first milestone should be a working prototype capable of:

1. Opening large PDF files.
2. Displaying the first page quickly.
3. Navigating through thousands of pages.
4. Zooming without blocking the interface.
5. Rendering pages asynchronously.
6. Managing a bounded page cache.
7. Printing documents efficiently.

The prototype must be benchmarked against qpdfview before the project expands into advanced functionality.

---

## Performance Benchmarks

Performance comparisons are mandatory.

Both applications should be tested with the same documents, hardware, operating system, Poppler version and printing configuration whenever possible.

### Benchmark test set

| Document | Purpose |
|---|---|
| 10-page PDF | Basic opening performance |
| 500-page PDF | Navigation and memory |
| 2,000-page PDF | Large document handling |
| 5,000-page PDF | Scalability and stability |
| 500 MB PDF | Memory and I/O behavior |
| Image-heavy PDF | Rendering performance |
| Font-heavy PDF | Typography and rendering |
| Password-protected PDF | Authentication |
| 1,000-page PDF | Printing performance |

### Metrics

- Time to first visible page.
- Total document initialization time.
- Page navigation latency.
- Zoom response time.
- Text search latency.
- RAM usage at idle.
- Peak RAM usage.
- CPU usage during rendering.
- Cache hit rate.
- Print submission time.
- Total print processing time.

The primary goal is to avoid significant performance regressions compared with qpdfview.

Absolute performance targets should be defined after measuring the reference application on the target hardware.

---

## Build Requirements

The development environment requires:

- Rust toolchain and Cargo.
- GTK4 development libraries.
- libadwaita development libraries.
- Poppler GLib development libraries.
- Cairo development libraries.
- GLib and GIO.
- CUPS development libraries where required.

### Example dependencies on Arch Linux / BigLinux

```bash
sudo pacman -S \
    rust \
    cargo \
    gtk4 \
    libadwaita \
    poppler-glib \
    cairo \
    glib2 \
    cups
```

Additional development packages may be required depending on the selected Rust bindings and build configuration.

### Build

```bash
cargo build
```

Run the app:

```bash
cargo run
```

Run the unit and PDF integration tests:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

The application currently needs Rust 1.92 or newer, GTK4 4.10 or newer, libadwaita 1.4 or newer, Poppler GLib 26.1 or newer, Cairo, and a CUPS `lp` client for printing. Native package names vary by distribution; Arch Linux/BigLinux provide the dependencies as `gtk4`, `libadwaita`, `poppler-glib`, `cairo`, and `cups`.

---

## Development Guidelines

### Rust

- Prefer safe Rust whenever possible.
- Keep unsafe FFI code isolated and documented.
- Define clear ownership rules for native objects.
- Avoid unnecessary allocations and copies.
- Use explicit error handling.
- Keep modules focused and independently testable.

### GTK4

- Keep GTK operations on the main thread.
- Avoid blocking the GLib main loop.
- Use GTK-native actions and shortcuts.
- Separate UI state from PDF processing.
- Prefer native GTK components where appropriate.

### PDF engine

- Do not implement PDF parsing from scratch.
- Keep Poppler integration isolated.
- Validate backend capabilities before implementing dependent features.
- Avoid rendering pages that are not needed.

### Performance

- Benchmark before optimizing.
- Measure memory usage.
- Limit background concurrency.
- Cancel obsolete tasks.
- Avoid full-document rasterization.
- Test with large and complex documents.

### Code quality

- Use `cargo fmt`.
- Use `cargo clippy`.
- Add unit and integration tests.
- Document public modules.
- Keep dependencies justified and minimal.

---

## Licensing

The project's final license must be established after reviewing the licensing obligations of the reference implementation and all selected dependencies.

The qpdfview source code and Poppler have GPL licensing considerations.

Before distributing gpdfview binaries or source code, the project must verify:

- Applicable GPL obligations.
- Copyright and attribution requirements.
- Third-party license notices.
- Distribution of corresponding source code where required.
- Compatibility of all dependencies and bindings.

The final license declaration should be included in the repository's `LICENSE` file.

---

## Contributing

Contributions are welcome once the initial architecture and development workflow are established.

Areas where contributions may be useful include:

- Rust and GTK4 development.
- Poppler integration.
- PDF rendering.
- Printing and CUPS integration.
- Performance profiling.
- UI design.
- Testing with large PDF documents.
- Packaging for Linux distributions.

Before implementing major features, contributors should discuss the architecture and intended behavior to avoid unnecessary duplication or incompatible approaches.

---

## Project Vision

**gpdfview aims to combine the proven efficiency of qpdfview with the safety and modularity of Rust and the native desktop integration of GTK4/libadwaita.**

The project will prioritize performance, functional completeness, memory efficiency and reliable printing.

The ultimate objective is simple:

**A modern native Linux PDF viewer that remains as fast and practical as the application that inspired it.**