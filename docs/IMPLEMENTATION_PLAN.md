# GPDFVIEW — Implementation Plan by Blocks

**Created from the 2026-11-03 Block 0 audit.** This plan keeps the ten blocks requested by the project owner. The only sequencing adjustment recommended is to pull the highest-risk performance and Poppler-threading work forward immediately after Block 1, because synchronous PDF operations currently affect search, thumbnails and outline loading.

## Execution contract

- Execute one block per approved cycle.
- Before each block: diagnose the relevant code and list the files/risks.
- Implement only that block's scope; do not rewrite unrelated modules.
- Run formatting, compilation, tests and clippy in a configured Rust/GTK/Poppler environment.
- Run UI/manual tests when a code-only test is insufficient.
- Update `README.md`, `docs/FEATURE_PARITY.md` and architecture notes when behavior changes.
- Report exact commands, pass/fail results, skipped checks and known limitations.
- Stop after the block report. Do not begin the next block without explicit authorization.

## Recommended order

| Execution order | Original block | Priority | Reason / dependency |
|---:|---|---|---|
| 0 | Audit (completed) | — | Establish source-of-truth docs; no application implementation was changed |
| 1 | Block 1 — Stability and architecture | P0 | Fix active-tab routing, identity, state ownership, error handling and tests |
| 2 | Block 7 — Performance and large PDFs | P0 | Search/thumbnail/outline synchrony and Poppler access policy are prerequisites for safe growth |
| 3 | Block 2 — PDF viewing and navigation | P1 | Verify the existing layout modes and complete interaction gaps |
| 4 | Block 3 — Tabs and document management | P0/P1 | Complete session behavior after stable active-document identity |
| 5 | Block 4 — Sidebar and document structure | P1 | Async/virtualized thumbnails and robust outline integration |
| 6 | Block 5 — Search and text selection | P1 | Async cancellable global search, highlighting and clipboard behavior |
| 7 | Block 6 — Professional printing | P1 | Fix scaling, destination/media selection and real backend verification |
| 8 | Block 8 — UX and settings | P2 | Complete shortcuts, menus/toolbars, accessibility and preferences |
| 9 | Block 9 — Security and robustness | P0 before release | Passwords, hostile/corrupt input, resource limits and failure recovery |
| 10 | Block 10 — Tests, benchmarks and release | P0 before release | Reproducible parity evidence, packaging, license and final regression pass |

The block numbers remain those from the project owner's directive. The execution order moves the performance block earlier; it does not remove or merge any block.

## Block 1 — Stability and architecture

**Goal:** establish a safe, predictable base without rewriting the app.

### Audit targets

- `src/window.rs`: shared actions and current-controller dispatch; sidebar/title/status updates on notebook page change; print signal connection lifetime.
- `src/document_tabs.rs`: stable tab identity and metadata synchronization under reorder/close/restore.
- `src/document_actions.rs`: opening multiple documents and ownership of controllers/workers.
- `src/app.rs`: application lifecycle, GApplication open handling and shared state.
- `src/session.rs`: persistence API and silent I/O errors.
- `src/poppler_lock.rs` plus all Poppler call sites: define one concurrency/ownership contract.

### Acceptance criteria

- Every global action resolves exactly one active document at activation time.
- Opening, switching, reordering, closing and restoring tabs cannot target the wrong PDF.
- A single shared print action has one signal handler and prints only the active document.
- All supplied GApplication open files are handled, each in its own tab.
- UI widgets and GTK objects remain owned by the main thread.
- Persistence errors are returned or logged; no silent write failures on normal paths.
- `cargo fmt`, `cargo check`, unit tests and clippy pass in a provisioned environment.
- Regression tests cover 2+ tabs, tab reorder, close/undo, print target and multi-file open.

## Block 7 — Performance and large PDFs (execute after Block 1)

**Goal:** keep opening and reading responsive for hundreds or thousands of pages.

### Work items

- Define Poppler document ownership, locking and worker boundaries.
- Move full-document search, outline extraction and thumbnail rendering off GTK main thread.
- Add cancellation/generation tokens for obsolete search and render work.
- Keep visible-page work higher priority than prefetch and thumbnails.
- Measure whether per-tab renderer/printer workers should become bounded shared pools.
- Retain separate, bounded page and thumbnail caches; expose hit/miss/eviction counters for profiling.
- Ensure workers shut down when documents/controllers close.
- Avoid loading/rasterizing all pages at open time.

### Acceptance criteria

- Opening a large PDF shows the first page without waiting for thumbnails or document-wide search.
- Scrolling and tab switching do not block the GTK main loop during expensive work.
- Rapid navigation cancels or supersedes stale render work.
- Memory use remains bounded and cache limits are explicit.
- Tests cover cancellation, stale generations, worker shutdown and cache eviction.
- A baseline measurement captures actual gpdfview process RSS and the first page's rendered/presented timestamps.

## Block 2 — PDF viewing and navigation

**Goal:** match qpdfview's core reading behavior.

- Verify single-page, continuous, two-page and split layouts with mixed page dimensions.
- Complete page navigation, page number entry, zoom, fit width/page and rotation.
- Define continuous layout scroll anchoring and visible-page tracking.
- Define facing-page cover/odd-page behavior and split pane synchronization.
- Verify keyboard, wheel and fullscreen/presentation behavior.

**Acceptance criteria:** all modes can be switched repeatedly without blank pages, stale surfaces, lost scroll position or cross-document state leakage; interaction tests cover boundary pages and mixed-size pages.

## Block 3 — Tabs and document management

**Goal:** robust multi-document operation.

- Finish active document/controller registry and per-tab state.
- Open all selected files; support repeated open and duplicate path policy.
- Reorder, close, reopen-last-closed, and restore multiple tabs.
- Persist tab ordering and each document's page/zoom/rotation/layout.
- Detect external file changes and deleted/unreadable documents.
- Ensure workers are cancelled/released when tabs close.

**Acceptance criteria:** a test scenario with at least 10 tabs can switch/reorder/close/restore without title/path/state mismatch; startup restores the documented session behavior.

## Block 4 — Sidebar and document structure

**Goal:** reliable thumbnail, outline and bookmark navigation.

- Replace synchronous thumbnail generation with a prioritized async queue.
- Virtualize thumbnail widgets; do not instantiate every page in a huge document.
- Preserve separate thumbnail cache and bounded memory.
- Load outlines asynchronously and handle nested, empty, remote or unsupported actions safely.
- Keep recent documents, favorites and page bookmarks synchronized with the active document.
- Persist and restore panel width/visibility if this is part of product scope.

**Acceptance criteria:** sidebar remains responsive for a multi-thousand-page PDF; scrolling the thumbnail list requests only relevant pages; selection navigates to the correct document/page.

## Block 5 — Search and text selection

**Goal:** fast, clear document-wide text workflows.

- Rename UI labels to accurately indicate document-wide search.
- Search asynchronously, incrementally and cancellably.
- Add progress/partial results for long files, next/previous match, result count and stable ordering.
- Preserve highlights across page navigation and clear stale highlights when query/document changes.
- Add selection/copy support if not already complete.
- Distinguish text-based PDFs from image-only scans; treat OCR as a separate optional subsystem.

**Acceptance criteria:** a multi-thousand-page fixture does not freeze the interface; cancelling or changing a query prevents old results from replacing newer results.

## Block 6 — Professional printing

**Goal:** predictable output across CUPS/IPP backends.

- Fix/verify fallback scale so it is applied exactly once.
- Add printer selection and expose supported paper/media/orientation options.
- Validate page ranges, copies, orientation and scale.
- Prefer original PDF submission when backend accepts it; use Cairo fallback only for a justified compatibility case.
- Avoid full-document rasterization and bound temporary storage.
- Surface submission, backend and failure status to the user.

**Acceptance criteria:** geometry tests pass; test jobs are validated with a CUPS test queue and at least one real printer where available; failures do not leave orphaned temporary PDFs.

## Block 8 — User experience and settings

**Goal:** complete a coherent native desktop workflow.

- Unify menu, toolbar and keyboard actions through the active-document command layer.
- Decide whether custom toolbars are in scope; if so, add a model and persistence.
- Complete shortcut customization and conflict detection.
- Improve document properties, recent/favorite management, status text and progress feedback.
- Validate accessibility, focus order, keyboard-only operation, high-DPI and narrow-window behavior.

**Acceptance criteria:** visible controls have real actions; settings persist and validate; the UI remains consistent across empty, single-tab and multi-tab states.

## Block 9 — Security and robustness

**Goal:** treat PDFs and metadata as untrusted input.

- Add password-protected document flow.
- Test corrupt, truncated, malformed and unusually large PDFs.
- Set sensible memory/task limits; recover from worker failure and cancellation.
- Validate paths and temporary files; avoid unsafe external action execution.
- Review external links/actions and document-originated behavior.
- Add fuzzing or corpus-based parser/render regression tests where practical.

**Acceptance criteria:** bad input produces a controlled error, no crash or unbounded memory growth; temporary files and workers are cleaned up after errors.

## Block 10 — Tests, benchmarks and release

**Goal:** produce reproducible evidence before claiming parity or performance.

- Replace current benchmark script; it samples the `timeout` wrapper after a fixed delay and is not a reliable app-RSS/first-page measurement.
- Measure first-rendered and first-presented page, sequential/random page latency, memory peak/idle, CPU, cache behavior and print submission.
- Compare qpdfview and gpdfview on the same machine, files, backend, display server and cold/warm conditions.
- Record exact hashes and runtime environment.
- Run full formatting/check/test/clippy/build matrix and GUI smoke tests.
- Complete license/attribution review; update packaging, desktop entry, icons and release artifacts.

**Acceptance criteria:** repeatable benchmark data, documented hardware/software, no known P0 defects, complete license decision and clean release build.

## Current blockers to track from the audit

See [`PROJECT_AUDIT.md`](PROJECT_AUDIT.md) for detailed code evidence. The immediate blockers are: active-tab command routing, tab reorder metadata, GApplication multi-file open, Poppler threading policy, synchronous UI work, print fallback scaling, silent persistence errors, invalid benchmark methodology and Poppler/license compatibility review.
