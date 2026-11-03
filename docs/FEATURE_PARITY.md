# Feature parity: gpdfview vs qpdfview

**Audit date:** 2026-11-03  
**Source of truth:** this matrix supersedes the older root-level matrix and historical sprint notes.  
**Scope note:** statuses describe source-level evidence in the audited ZIP. `IMPLEMENTADO` means a code path exists for the feature; it does not imply full user-interface, regression, performance or backend validation. Runtime verification is still required.

## Status definitions

- **IMPLEMENTADO** — implementation exists and the basic behavior is represented in source/tests.
- **PARCIAL** — a meaningful implementation exists, but feature scope, integration or validation is incomplete.
- **AUSENTE** — no implementation path was found in the inspected repository.
- **COM PROBLEMAS** — the feature exists, but a correctness/integration defect was found.
- **NÃO APLICÁVEL** — deliberately outside the declared PDF-focused product scope; this status must be an explicit product decision.

## Core viewing and navigation

| qpdfview capability / expected behavior | gpdfview status | Evidence in gpdfview | Remaining work |
|---|---|---|---|
| Open a local PDF | IMPLEMENTADO | `src/app.rs`, `src/document_actions.rs`, `src/engine.rs` | Runtime tests with invalid, protected and inaccessible files |
| Open multiple PDFs in separate tabs through file picker | IMPLEMENTADO | `src/document_actions.rs::show_open_dialog` uses `open_multiple`; `src/document_tabs.rs` | Verify many-file selection and tab activation |
| Open multiple files from a GApplication open request | COM PROBLEMAS | `src/app.rs::run` uses `files.first()` | Iterate and open every supplied file |
| Tabbed documents | COM PROBLEMAS | `src/document_tabs.rs`; `src/document_actions.rs`; `src/window.rs` | Route all shared actions to active tab; do not accumulate print handlers |
| Reorder tabs and close correct document | COM PROBLEMAS | `src/document_tabs.rs` keeps parallel title/path vectors and allows reorderable tabs | Synchronize metadata to notebook order or bind identity to page |
| Page navigation and page-number entry | IMPLEMENTADO | `src/navigation.rs`, `src/window.rs` | GUI boundary tests and invalid input handling |
| Zoom in/out and custom zoom value | IMPLEMENTADO | `src/navigation.rs`, `src/window.rs` | High-DPI and rapid zoom regression tests |
| Fit width / fit page | IMPLEMENTADO | `src/layout.rs`, `src/navigation.rs` | Mixed page sizes and viewport changes |
| Page rotation | IMPLEMENTADO | `src/navigation.rs`, `src/viewer_state.rs` | Verify all layout modes and per-document restore |
| Single-page layout | IMPLEMENTADO | `src/viewer_state.rs`, `src/window.rs` | Visual/runtime verification |
| Continuous scrolling | PARCIAL | `src/viewer_state.rs`, `src/window.rs`, render cache/prefetch paths | Verify long documents, scroll anchoring and pages missing from cache |
| Two-page/facing-page layout | PARCIAL | `LayoutMode::TwoPage`, `src/window.rs` | Verify odd/even page alignment, mixed page dimensions and RTL policy |
| Split view | PARCIAL | `SplitViewMode`, `src/viewer_state.rs`, `src/window.rs`, `src/ui_commands.rs` | Independent pane navigation/scroll state and clear sync semantics |
| Fullscreen | IMPLEMENTADO | `src/window.rs`, `src/app.rs` | Manual desktop and keyboard regression tests |
| Presentation mode | IMPLEMENTADO | `src/app.rs`, `src/window.rs`, `src/navigation.rs` | Verify navigation, exit behavior and multi-monitor behavior |
| Document properties | PARCIAL | `src/dialogs.rs`, `src/engine.rs` | Broader metadata, page size/count, security and font details |

## Search, navigation panels and document state

| Capability | gpdfview status | Evidence in gpdfview | Remaining work |
|---|---|---|---|
| Search text in current page | IMPLEMENTADO | `src/engine.rs::find_text_rectangles_in_page` | Current UI actually performs whole-document search; make wording precise |
| Search across full document | PARCIAL | `src/engine.rs::find_text_rectangles_in_document`, `src/rendering.rs` | Synchronous all-page scan; async incremental results, cancellation, progress and stress tests |
| Navigate next/previous search result | IMPLEMENTADO | `src/rendering.rs`, `src/app.rs`, `src/viewer_state.rs` | Cross-tab focus and stale query cancellation |
| Highlight search results | IMPLEMENTADO | `src/viewer_state.rs`, `src/window.rs` | Visual tests for multiple matches and page changes |
| Thumbnail page navigator | PARCIAL | `src/sidebar.rs`, `src/engine.rs::render_page_thumbnail`, separate 24 MiB cache | Render is synchronous; only a limited window of pages is built; virtualize and load asynchronously |
| PDF outline/bookmarks | PARCIAL | `src/engine.rs::document_outline`, `src/sidebar.rs` | Only Go-To destinations become navigable entries; async loading and nested/edge-case tests |
| Recent documents | IMPLEMENTADO | `src/session.rs`, `src/sidebar.rs` | Error handling, eviction tests and inaccessible-file UX |
| Application favorites | IMPLEMENTADO | `src/session.rs`, `src/sidebar.rs` | Verify add/remove/duplicate handling with active-tab routing |
| User page bookmarks | IMPLEMENTADO | `src/session.rs`, `src/sidebar.rs` | Verify persistence and navigation across multiple tabs |
| Persist page, zoom, rotation and layout per document | IMPLEMENTADO | `src/session.rs`, `src/app.rs` | Atomic writes, versioned storage and file identity/invalidation |
| Restore every previously open tab/session | PARCIAL | `src/session.rs`, `src/app.rs` | Current startup restore is centered on the last document, not a full multi-tab session |
| Settings for default layout, split sync and zoom step | IMPLEMENTADO | `src/session.rs`, `src/dialogs.rs` | Schema/versioning and malformed-value tests |
| Configurable keyboard shortcuts | PARCIAL | `src/keyboard.rs`, `src/dialogs.rs`, `src/session.rs` | Settings UI exposes a subset; validate conflicts and full action coverage |
| Configurable toolbars | AUSENTE | No equivalent model/UI found | Add action model and persistence if desired |

## Printing and document formats

| Capability | gpdfview status | Evidence in gpdfview | Remaining work |
|---|---|---|---|
| Send original PDF to CUPS (`lp`) | PARCIAL | `src/printing.rs::submit_to_cups` | Uses default destination; no printer chooser; backend compatibility and real jobs unverified |
| Rendered-PDF fallback using Poppler/Cairo | PARCIAL | `src/printing.rs::render_for_printing` | Verify output page dimensions, memory behavior and double-scaling risk |
| Select page range and copies | IMPLEMENTADO | `src/printing.rs`, print dialog in `src/app.rs` | Validate against actual printers/IPP backends |
| Printer selection, paper size, advanced media/quality | AUSENTE | No complete printer destination/media selection flow found | Integrate printer capabilities from CUPS/GTK print APIs |
| PDF annotations | AUSENTE | No annotation editing path found | Decide scope and Poppler/backend feasibility |
| Interactive PDF forms | AUSENTE | No form interaction/editing path found | Technical spike and security/accessibility assessment |
| SyncTeX | AUSENTE | No SyncTeX integration found | Optional extension after core stability |
| DjVu/PostScript backends or plugins | AUSENTE | No additional document backend/plugin layer found | Either explicitly exclude from product scope or define a backend abstraction |
| Command-line open at a specified page / qpdfview-style `file#page` | AUSENTE | No equivalent CLI parser found in inspected source | Add if workflow parity requires it |

## Reliability, performance and security

| Capability / quality requirement | gpdfview status | Evidence | Remaining work |
|---|---|---|---|
| On-demand page rendering worker | IMPLEMENTADO | `src/engine.rs::PdfRenderer` | Verify worker shutdown, cancellation and multiple documents |
| Bounded page cache | IMPLEMENTADO | `src/cache.rs`, `src/app.rs` (128 MiB cap) | Measure cache hit rate and memory pressure |
| Separate thumbnail cache | IMPLEMENTADO | `src/sidebar.rs` (24 MiB cap) | Async generation and virtualization |
| Prefetch adjacent pages | IMPLEMENTADO | `src/engine.rs` pending/prefetch queue | Tune with measurement; prevent wasted work under rapid navigation |
| Poppler concurrency contract | COM PROBLEMAS | `src/poppler_lock.rs` exists, but not every inspected Poppler operation uses it consistently | Centralize policy and document object/thread ownership |
| Search/thumbnail/outline not blocking GTK | COM PROBLEMAS | Synchronous call paths in `src/rendering.rs` and `src/sidebar.rs` | Move expensive work to cancellable workers |
| Robust persistence and surfaced I/O errors | COM PROBLEMAS | Silent `let _ = fs::write(...)` in `src/session.rs` | Atomic writes, schema version, error telemetry and recovery tests |
| Print scaling behavior | COM PROBLEMAS | Scale is applied while creating fallback PDF and again in CUPS options | Confirm and remove double scaling |
| Benchmark first-visible-page latency and app RSS | COM PROBLEMAS | `scripts/benchmark.sh` samples `timeout` wrapper after fixed sleep | Replace harness; invalidate old TSV as a comparable baseline |
| PDF password prompt | AUSENTE | Loading paths pass no password to `Document::from_file` | Implement secure password flow if required |
| Handling malformed PDFs and resource limits | PARCIAL | Some errors are returned, but comprehensive hostile-file/resource tests are absent | Fuzz/safety tests, limits and robust error UX |

## qpdfview reference scope

The comparison uses qpdfview's published Debian AppStream feature metadata and manual as a public feature reference. Those list document tabs, outline/properties/thumbnail panels, zoom/rotation/fit modes, continuous/multi-page display, fullscreen/presentation, search, persistent per-file settings, configurable shortcuts/toolbars, and optional integrations such as SyncTeX and additional document types. Not every qpdfview extension needs to be part of gpdfview's initial scope; product scope should be explicit.

References:

- <https://appstream.debian.org/trixie/main/metainfo/qpdfview.html>
- <https://www.gsp.com/cgi-bin/man.cgi?section=1&topic=QPDFVIEW>
- <https://packages.debian.org/sid/qpdfview>

For technical defects and priorities, see [`PROJECT_AUDIT.md`](PROJECT_AUDIT.md). For the ordered work sequence, see [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md).
