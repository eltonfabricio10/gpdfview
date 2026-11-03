# GPDFVIEW — Technical Audit (Block 0)

**Audit date:** 2026-11-03  
**Audited snapshot:** Git commit `98914318154052f7a965a5d74f8f4aa6167e45b5` (`2026-11-03 09:06:14 -0300`)  
**Repository:** `https://github.com/eltonfabricio10/gpdfview`  
**Scope:** static inspection of the supplied ZIP; no application source code was changed.

## 1. Executive summary

The supplied project is beyond an initial prototype. It contains a modular Rust/GTK4 application with PDF rendering, per-document viewer state, tabs, navigation, search, thumbnails, outline extraction, bookmarks, settings/session persistence, and CUPS printing with a rendered-PDF fallback.

The most important risks are not simply missing feature checkboxes. Several existing features are not yet reliably integrated across multiple tabs, some Poppler operations remain synchronous on the GTK thread, the current benchmark script does not measure the application process or first-page display, and the repository's license declaration needs review against the native Poppler dependency before distribution.

**Current recommendation:** do not begin broad feature expansion yet. The first implementation work should establish a reliable active-document model, fix tab bookkeeping, define safe Poppler access boundaries, make expensive operations asynchronous, and replace the benchmark methodology. The detailed ten-block plan is in [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md).

## 2. Snapshot and audit method

The ZIP was extracted to a working copy. Git metadata in that copy reports a clean working tree at the audited commit. The inspection covered:

- `Cargo.toml` and `Cargo.lock`;
- all Rust source files under `src/`;
- root documentation and `dist/README.md`;
- benchmark scripts and historical measurement files;
- PDF fixtures under `tests/fixtures/`;
- tab, session, rendering, search, Poppler, printing, and window-management paths.

The source tree contains 19 Rust source files (including `main.rs` and the `test_poppler` binary) and approximately 6,131 lines across the inspected Rust files. A grep-based inventory finds 44 `#[test]` declarations. This is a source inventory only: those tests were **not executed** during this audit.

### Environment limitations

The audit container has `pkg-config`, but does not have `cargo` or `rustc`, and the GTK4, libadwaita, Poppler GLib, GLib/GObject and Cairo development packages are not available through `pkg-config`. Consequently, this audit could not run:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

It also could not launch the GUI or validate an actual CUPS printer. Findings below are based on source inspection and existing repository artifacts; runtime behavior still needs verification on a configured Linux desktop.

## 3. Project inventory

### Rust modules

| Module | Observed responsibility |
|---|---|
| `src/main.rs` | Application entry point and module declarations |
| `src/app.rs` | Application/controller state, actions, settings integration and tests |
| `src/window.rs` | GTK window, toolbar, drawing area, document controller and signal wiring |
| `src/document_actions.rs` | File selection and opening documents in tabs |
| `src/document_tabs.rs` | Notebook tab labels, paths, closing and recently closed items |
| `src/engine.rs` | Poppler document loading, rendering, thumbnails, outline, search and renderer worker |
| `src/rendering.rs` | Search interaction and render-result handling |
| `src/cache.rs` | Bounded LRU-style surface cache |
| `src/navigation.rs` | Page navigation and zoom operations |
| `src/layout.rs` | Page layout/geometry calculations |
| `src/viewer_state.rs` | Per-viewer state and layout modes |
| `src/sidebar.rs` | Thumbnail, outline, search, recent, favorites and bookmark panels |
| `src/printing.rs` | Print worker, CUPS submission and Cairo PDF fallback |
| `src/session.rs` | App settings, recents, favorites, bookmarks, window and per-document persistence |
| `src/keyboard.rs` | Keyboard action mapping |
| `src/dialogs.rs` | Settings and document-properties UI |
| `src/ui_commands.rs` | UI command handlers |
| `src/ui_helpers.rs` | Shared UI construction helpers |
| `src/poppler_lock.rs` | Global Poppler access mutex |
| `src/bin/test_poppler.rs` | Poppler diagnostic executable |

### Other relevant files

- `Cargo.toml` / `Cargo.lock`: Rust manifest and locked dependency graph.
- `tests/fixtures/`: four PDF fixtures are present.
- `scripts/benchmark.sh`: current gpdfview launch/RSS script.
- `measure_qpdfview.sh`: qpdfview reference measurement helper.
- `scripts/package_release.sh`: basic release packaging script.
- `measurements/`: historical measurement output; not all entries use comparable methodology.
- Root planning files: `PARITY_MATRIX.md`, `NEW_FEATURE_ROADMAP.md`, `QPDFVIEW_PARITY_BLOCK_PLAN.md`, `NEXT_PHASE_SUMMARY.md`, `STAGE_1_SUMMARY.md`, `STAGE_2_SUMMARY.md`, and `PLANNING_ALIGNMENT_2026-11-02.md`.

## 4. Dependency and build review

The manifest declares Rust edition 2021 and these principal dependencies:

| Dependency | Manifest requirement | Role |
|---|---:|---|
| `gtk4` | `0.11` (`v4_10`, `v4_12`) | GTK4 UI |
| `libadwaita` | `0.9` | GNOME/libadwaita UI |
| `glib` / `gio` | `0.20` | GLib/GIO APIs |
| `cairo-rs` | `0.22` with `pdf` | Cairo rendering and PDF fallback |
| `poppler-rs` | `0.26` | Poppler Rust bindings |
| `once_cell` | `1` | Lazy/static initialization |
| `anyhow` | `1` | Error context |
| `tracing` / `tracing-subscriber` | `0.1` / `0.3` | Diagnostics/logging |

The lockfile contains both GLib/GIO 0.20 and 0.22 packages. This is not, by itself, proof of a defect, but the dependency graph should be inspected with `cargo tree -d` and `cargo tree -i glib` in a complete build environment. Avoid passing GLib/GObject values across incompatible crate versions; keep FFI boundaries explicit.

The manifest also defines a `test_poppler` binary. Keep it as a developer diagnostic tool, and ensure release packaging does not accidentally present it as the main application.

## 5. Confirmed implementation areas

The following capabilities have corresponding implementation paths in the source. Their feature status is refined in [`FEATURE_PARITY.md`](FEATURE_PARITY.md).

- PDF loading through Poppler.
- A dedicated page-rendering worker per viewer controller, with a pending visible request and queued prefetch work.
- A bounded page cache (`128 MiB` limit in `app.rs`).
- Separate thumbnail cache (`24 MiB` limit in `sidebar.rs`).
- Page navigation, page-number entry, zoom, fit width/page and rotation.
- Single-page, continuous, two-page and split-view state/render paths.
- Notebook-based multiple tabs with reorderable tab widgets.
- Global document text-search code path and result navigation/highlighting.
- Thumbnail rendering, PDF outline extraction, recent documents, favorites and app bookmarks.
- Per-document persisted page, scale, rotation and layout values.
- Fullscreen and presentation-mode actions.
- CUPS submission via `lp`, plus a Cairo/Poppler fallback that generates a temporary PDF.
- A declared set of 44 unit-test functions/attributes in the Rust source.

A code path does not establish production readiness. In particular, end-to-end interaction across multiple tabs, actual printing, large-document latency and memory use remain unverified in this environment.

## 6. Findings and risks

Priority labels: **P0** blocks reliable everyday use or release confidence; **P1** materially limits parity/performance; **P2** is a lower-priority parity or product gap.

### P0 — Active document routing is incomplete

**Evidence:** `src/window.rs`, especially the menu/header callbacks and `create_document_controller`; `src/sidebar.rs`; `src/document_actions.rs`.

- Several window-level menu actions are created around the initial controller rather than resolving the currently selected notebook page.
- The shared print button receives a new `connect_clicked` closure for each document controller. Each closure captures that controller, so a single shared button can accumulate handlers for multiple tabs.
- The sidebar is constructed against the initial controller and is not evidently rebound to the active tab on notebook page changes.
- There is no `Notebook::switch-page` handler found in the inspected source.

**Impact:** after opening more than one PDF, visible tab selection and the target of global actions can diverge. This is a correctness issue, not merely a UI polish issue.

**Recommended correction:** create a single active-document/controller registry keyed by notebook page (or a stable document ID). Connect shared window actions exactly once and resolve the active controller at activation time. Update sidebar, title, status, print and menu state on `switch-page`.

### P0 — Reorderable notebook tabs can desynchronize metadata

**Evidence:** `src/document_tabs.rs` stores titles and paths in parallel vectors, sets each page reorderable, and later indexes those vectors by notebook page number. No page-reordered synchronization handler was found.

**Impact:** after dragging tabs, closing a tab may remove the wrong stored title/path. Recently-closed restoration can then restore an incorrect document.

**Recommended correction:** store tab metadata with the page/controller itself or update a single ordered collection whenever GTK reorders pages. Add integration tests for reorder → close → restore.

### P0 — Opening several files through GApplication drops all but the first

**Evidence:** `src/app.rs`, `app.connect_open`; it reads `files.first()` and opens only that item. The file picker path uses `open_multiple`, so the application has two different multi-file behaviors.

**Impact:** users passing multiple files from the file manager or command line should not assume they will all open.

**Recommended correction:** iterate through all supported local files and open each into a separate tab; handle unsupported/non-local files individually.

### P0 — License declaration must be reconciled before distribution

**Evidence:** repository `LICENSE` is MIT; the application links to native Poppler through `poppler-rs`/Poppler GLib.

Poppler's core native library is distributed under GPL terms (the exact applicable version and notices depend on the installed Poppler release and package). The Rust binding crate's own license does not replace the license of the native library being linked. The MIT declaration in the repository therefore requires a release/legal review and may be incompatible with the intended distribution model. qpdfview itself is GPL-2.0-or-later in Debian's AppStream metadata.

**Recommended correction:** review the exact Poppler version, linking model, all transitive/native dependencies and the desired distribution model. Update the project license and third-party notices only after this review. Do not distribute binaries under an unqualified MIT claim until resolved.

### P1 — Poppler operations are not consistently isolated from the UI thread

**Evidence:** `src/engine.rs`, `src/rendering.rs`, `src/sidebar.rs`, and `src/poppler_lock.rs`.

- `find_text_rectangles_in_document` iterates through every page and calls Poppler text search synchronously; `rendering::search_current_page` invokes it directly from the search action path.
- Thumbnail generation is called synchronously when sidebar thumbnail widgets are built.
- Outline extraction is also called synchronously from sidebar population.
- `poppler_lock.rs` defines a global mutex, but inspected Poppler call sites do not all acquire it consistently (notably document outline and document-wide search paths).

**Impact:** large PDFs can stall the GTK main loop. Inconsistent synchronization also makes the concurrency contract difficult to reason about.

**Recommended correction:** define one explicit Poppler access policy. Keep GTK objects on the main thread; execute expensive Poppler work in workers with owned inputs/outputs; guard all shared native access consistently or use isolated Poppler document instances where justified. Add cancellation and generation IDs to search and thumbnail tasks.

### P1 — Worker count grows linearly with open tabs

**Evidence:** `src/document_actions.rs` calls `new_worker_pair()` for every new tab; `src/app.rs::new_worker_pair` creates one `PdfRenderer` and one `PrintService`.

**Impact:** every tab can add two long-lived threads even when idle. This can increase memory and scheduling overhead in sessions with many documents.

**Recommended correction:** measure first, then consider an application-level bounded executor or shared worker pools with per-document task IDs and cancellation. Do not merge workers until Poppler safety boundaries are explicit.

### P1 — Search label and search behavior disagree

**Evidence:** `src/window.rs` sets the search entry placeholder to “Search in page”, while `src/rendering.rs` calls `find_text_rectangles_in_document` and reports matches in the whole document.

**Impact:** UI wording under-specifies the feature, while the expensive whole-document search is synchronous and lacks a visible progress/cancel flow.

**Recommended correction:** label it “Search in document”; implement asynchronous incremental search, cancellation, progress/partial results, and explicit match navigation semantics.

### P1 — Thumbnail and outline features are present but synchronous and constrained

**Evidence:** `src/sidebar.rs` calls `render_page_thumbnail` synchronously and builds a limited range of pages around the current page; `document_outline` extracts navigable GOTO_DEST entries.

**Impact:** the project has actual thumbnail rendering and outline extraction, contrary to older documentation, but thumbnails are not a complete virtualized all-page navigator and outline extraction is not a full representation of every possible PDF action/destination.

**Recommended correction:** use a virtualized list/grid and separate async thumbnail queue/cache. Preserve unsupported outline actions as disabled/unsupported entries or document that only internal Go-To destinations are navigable.

### P1 — Print fallback may apply scaling twice

**Evidence:** `src/printing.rs::render_for_printing` scales the generated page dimensions and Cairo context using `scale_percent`; the generated PDF is then passed to `submit_to_cups`, which also sends the same `scaling=` option to `lp`.

**Impact:** the fallback path may apply user scaling twice, depending on the CUPS filter/backend behavior. This has not been runtime-verified.

**Recommended correction:** choose one scaling stage for the fallback path and test with known page geometry on a real CUPS backend. Add automated tests for generated PDF dimensions and requested scale.

### P1 — Persistence silently discards write failures

**Evidence:** `src/session.rs` repeatedly uses `let _ = fs::write(...)` for settings, recent files, window state and document state.

**Impact:** permission, disk-full or I/O errors are silently ignored; users may believe settings/session were saved when they were not. Writes are not visibly atomic and the plain-text format lacks schema/version migration.

**Recommended correction:** return/record errors, use atomic replace (temporary file + rename), add a format version, validate paths and values, and test recovery from partial/corrupt files.

### P1 — Benchmark methodology is not measuring first-page latency or app RSS

**Evidence:** `scripts/benchmark.sh` launches `timeout 12s APP`, stores the PID of the `timeout` wrapper, sleeps one second, and reads `/proc/$pid/status`. It records a fixed ~1 second launch interval and samples the wrapper rather than reliably sampling the GTK process. Killing the wrapper may also leave the child process running depending on process-group behavior.

The existing `measurements/gpdfview_benchmark.tsv` entries cluster around 1005 ms and RSS around a few MiB, which is not credible as a direct measurement of the full GTK application and should not be used as an app RSS baseline. qpdfview historical startup measurements also use a different proxy method.

**Recommended correction:** identify and sample the actual process (or use a dedicated benchmark harness), instrument the moment the first page is rendered and presented, use repeated cold/warm runs, capture peak RSS, record environment and compare both applications under identical conditions. Invalidate or relabel the current TSV as an obsolete/invalid baseline.

### P2 — Documentation has drifted from the implementation

**Evidence:** `README.md`, `PARITY_MATRIX.md`, `NEW_FEATURE_ROADMAP.md`, `NEXT_PHASE_SUMMARY.md`, and stage summaries disagree with `src/`.

Examples: README calls tabs/sidebar future work while they exist; current README claims missing thumbnails/outlines even though the implementation has thumbnail rendering and outline extraction; the parity matrix labels outline missing and per-document preferences in-memory only despite persistent page/scale/rotation/layout code. The stage summary also records an older dependency/test snapshot.

**Recommended correction:** make `docs/FEATURE_PARITY.md` the current source of truth; mark stage/phase documents as historical snapshots; link root README and root matrix to the new audit docs.

### P2 — qpdfview parity still has explicit gaps

Not found in the inspected code paths: SyncTeX integration, PDF annotations, interactive forms, configurable toolbars, integrated DjVu/PostScript backends/plugins, and richer command-line behavior such as opening a requested page. These should remain declared gaps unless scope is deliberately narrowed.

## 7. Benchmark status

The current benchmark artifacts are **not valid evidence of comparable performance**. They cannot support claims that gpdfview is faster/slower than qpdfview. A replacement protocol should measure at least:

1. process spawn → document parsed;
2. process spawn → first page rendered;
3. first page rendered → first page actually presented;
4. page-to-page latency (sequential and random);
5. peak and idle RSS;
6. CPU time and render queue depth;
7. cold-cache and warm-cache repetitions;
8. print-job submission and completion separately.

Record exact document hashes, application versions, OS/kernel, GTK, Poppler, CUPS, CPU/RAM, display backend and repetition count.

## 8. Required actions before Block 1 is considered complete

- [ ] Define and implement active-document routing for all shared actions and sidebar panels.
- [ ] Make tab identity stable across reorder, close and restore.
- [ ] Open all files in GApplication `open` requests.
- [ ] Define and document the Poppler threading/synchronization contract.
- [ ] Move whole-document search, outline and thumbnail work off the GTK main thread.
- [ ] Resolve the license review before any release/distribution.
- [ ] Repair print fallback scaling and verify on a CUPS test environment.
- [ ] Replace the invalid benchmark script and mark old measurements non-comparable.
- [ ] Run fmt/check/test/clippy and desktop smoke tests in a complete Rust/GTK/Poppler build environment.

## 9. References

- qpdfview feature metadata: <https://appstream.debian.org/trixie/main/metainfo/qpdfview.html>
- qpdfview manual: <https://www.gsp.com/cgi-bin/man.cgi?section=1&topic=QPDFVIEW>
- qpdfview package information: <https://packages.debian.org/sid/qpdfview>
- Poppler copyright/package licensing: <https://sources.debian.org/src/poppler/25.03.0-5+deb13u4/debian/copyright/>
- Poppler project README: <https://gitlab.freedesktop.org/poppler/poppler/-/blob/master/README.md>

External feature and license references were used as comparison material; the project source remains the authority for gpdfview implementation status.
