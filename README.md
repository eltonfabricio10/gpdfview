# gpdfview

**A native Linux PDF viewer built with Rust, GTK4, libadwaita, Poppler and Cairo.**

`gpdfview` aims to provide a fast, resource-conscious PDF reading workflow with feature parity goals inspired by [qpdfview](https://launchpad.net/qpdfview), while using a native GTK architecture rather than Qt.

> **Project status — audited 2026-11-03:** the repository contains a working application architecture with PDF rendering, page navigation, zoom, rotation, multiple layout modes, notebook tabs, document-wide text search, thumbnails, outline navigation, favorites/bookmarks, per-document view persistence, fullscreen/presentation actions and CUPS printing with a Cairo/Poppler fallback. Several integrations and performance paths are still partial or require runtime validation. This is not a claim of complete qpdfview parity.

## Current capabilities

| Area | Current implementation status |
|---|---|
| PDF loading and basic navigation | Implemented; runtime edge-case verification remains |
| Zoom, fit width/page and rotation | Implemented |
| Single-page / continuous / two-page layouts | Implemented in source; visual and long-document validation remains |
| Split view | Partial; pane behavior and synchronization need validation |
| Multiple tabs | Present, but active-tab routing and tab-reorder metadata have defects to fix |
| Document-wide text search and highlights | Present, but synchronous and unsuitable for very large files until moved to workers |
| Thumbnails and PDF outline | Present, but loading is synchronous and thumbnails are currently built for a limited page window |
| Recent files, favorites and page bookmarks | Present; persistence/error handling and multi-tab integration need hardening |
| Per-document page/zoom/rotation/layout persistence | Present; storage needs atomic writes, error reporting and format versioning |
| CUPS printing and rendered-PDF fallback | Present; printer selection/backend testing and fallback scaling require work |
| Fullscreen and presentation mode | Present in source; desktop regression testing remains |
| Annotations, interactive forms, SyncTeX, extra document backends | Not implemented in the audited snapshot |

The authoritative, evidence-based status table is [`docs/FEATURE_PARITY.md`](docs/FEATURE_PARITY.md). Critical findings are recorded in [`docs/PROJECT_AUDIT.md`](docs/PROJECT_AUDIT.md).

## Design goals

1. Display the first page quickly, without rendering every page at open time.
2. Keep the GTK main loop responsive while rendering, searching, building thumbnails or printing.
3. Use bounded caches and predictable worker lifetimes.
4. Keep per-document view state independent across tabs.
5. Prefer submitting the original PDF to CUPS when supported; use rendered fallback only when needed.
6. Treat malformed, protected and unusually large PDFs as untrusted input.
7. Keep the code idiomatic, modular and maintainable Rust.

## Technology stack

- **Rust** — application logic and state management.
- **GTK4** — native desktop interface.
- **libadwaita** — GNOME-style application components.
- **Poppler GLib / `poppler-rs`** — PDF parsing, text extraction and rendering.
- **Cairo** — page drawing and temporary PDF output.
- **CUPS (`lp`)** — current print submission path.
- **GLib/GIO** — application lifecycle, file APIs and main-loop integration.

No Qt dependency is intended for the gpdfview application.

## Architecture overview

The application has a main window and a `ViewerController` for each opened document. A controller owns its viewer state, drawing area, navigation controls, page cache and worker handles. `PdfRenderer` performs page rendering in a worker; `PrintService` queues print requests. The sidebar provides thumbnails, outline, search results, recents, favorites and page bookmarks. `session.rs` persists settings and document state.

Important integration work remains: shared window actions and sidebar state must consistently target the active tab; tab metadata must remain correct after reordering; expensive Poppler operations must not block GTK; and Poppler native access needs one documented concurrency policy.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the observed architecture and proposed boundaries.

## Performance strategy

The current code includes a bounded page cache (128 MiB per viewer), a separate thumbnail cache (24 MiB), visible-page render requests and adjacent-page prefetch. These are useful foundations, not proof that the current application already matches qpdfview's startup or navigation speed.

The current benchmark script does not reliably sample the gpdfview process or first-page presentation. Existing measurements must not be used as comparative performance evidence until the harness is replaced. The required benchmark plan is documented in [`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md), Block 7 and Block 10.

## Printing

The preferred path submits the original PDF to CUPS through `lp`. If that path fails, the current fallback uses Poppler and Cairo to generate a temporary PDF from the selected pages.

The fallback scale is applied during temporary-PDF generation and is also passed as a CUPS scaling option. This potential double-scaling behavior must be resolved and tested with a CUPS test queue or real printer before printing is considered production-ready.

## Project structure

```text
gpdfview/
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── docs/
│   ├── PROJECT_AUDIT.md
│   ├── FEATURE_PARITY.md
│   ├── IMPLEMENTATION_PLAN.md
│   └── ARCHITECTURE.md
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── window.rs
│   ├── document_actions.rs
│   ├── document_tabs.rs
│   ├── engine.rs
│   ├── rendering.rs
│   ├── cache.rs
│   ├── navigation.rs
│   ├── layout.rs
│   ├── viewer_state.rs
│   ├── sidebar.rs
│   ├── printing.rs
│   ├── session.rs
│   ├── keyboard.rs
│   ├── dialogs.rs
│   ├── ui_commands.rs
│   ├── ui_helpers.rs
│   ├── poppler_lock.rs
│   └── bin/test_poppler.rs
├── tests/fixtures/
├── scripts/
└── measurements/
```

The repository may contain historical planning and milestone documents. They are snapshots of earlier work; use `docs/FEATURE_PARITY.md` and `docs/IMPLEMENTATION_PLAN.md` for current implementation status and next steps.

## Build requirements

A configured Linux development environment needs:

- Rust toolchain (`rustc`, `cargo`);
- GTK4 development files matching the APIs enabled in `Cargo.toml`;
- libadwaita development files;
- Poppler GLib development files;
- Cairo development files;
- CUPS client tools (`lp`) for the current print path.

On Arch Linux / BigLinux, package names and versions can change; verify the current distribution package names before installing. Then build with:

```bash
cargo build --release
```

Run from the repository with:

```bash
cargo run --release -- path/to/document.pdf
```

Run available Rust tests with:

```bash
cargo test
```

Run static checks with:

```bash
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
```

The commands above describe the expected developer workflow. They are not a claim that the supplied audit environment successfully executed them.

## Development workflow

Development is organized into ten approved blocks. Only one block should be implemented per cycle. Every cycle must include diagnosis, scoped plan, implementation, tests, review, documentation update and a report. Do not mark work complete when checks were skipped or when a code path has not been verified.

See [`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md) for the block order and acceptance criteria.

## Licensing — release blocker to review

The repository currently contains an MIT `LICENSE`, while the application links to the native Poppler library. Poppler's native core is distributed under GPL terms; the exact obligations depend on the version and distribution package. The `poppler-rs` crate's own license does not replace the license of the native library.

Before distributing gpdfview binaries or source under the current declaration, review the exact Poppler release, all native/transitive dependencies, linking model, attribution and corresponding-source obligations. The repository's license may need to change to a compatible GPL license. This is a release/legal review item, not a conclusion that can be settled solely by the Rust binding metadata.

References:

- <https://sources.debian.org/src/poppler/25.03.0-5+deb13u4/debian/copyright/>
- <https://gitlab.freedesktop.org/poppler/poppler/-/blob/master/README.md>
- <https://appstream.debian.org/trixie/main/metainfo/qpdfview.html>

## Contributing

Contributions should be scoped to the active implementation block. Before submitting changes:

1. Read the audit and architecture documents.
2. Preserve existing working behavior.
3. Add regression tests for the changed behavior.
4. Run formatting, compilation, tests and clippy where the environment permits.
5. Document native dependencies, threading assumptions and performance implications.
6. Update the feature parity matrix when a capability's status changes.

## Project vision

The goal is a fast, reliable, resource-conscious Linux PDF viewer with a familiar qpdfview-level reading workflow and a maintainable Rust/GTK4 implementation. Performance claims must be supported by reproducible measurements, and parity claims must be supported by tested behavior rather than the existence of source files alone.
