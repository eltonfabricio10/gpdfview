# gpdfview Parity Matrix (vs qpdfview)

Last update: 2026-11-02

This matrix tracks planned parity from the planning document and maps each item to implementation status and validation artifacts.

## P0 — Essential Core

| Feature | Status | Evidence | Next Action |
|---|---|---|---|
| Open/close PDF | Implemented | src/document_actions.rs, src/app.rs | Keep regression tests for open/close flow |
| Multi-document tabs | Implemented | src/document_tabs.rs, src/window.rs | Add stress test for many tabs |
| Page navigation | Implemented | src/navigation.rs | Add jump-to-page boundary tests |
| Zoom in/out | Implemented | src/navigation.rs | Add high-DPI scenario checks |
| Fit width / fit page | Implemented | src/navigation.rs, src/layout.rs | Validate with mixed page sizes |
| Continuous layout | Implemented | src/viewer_state.rs, src/window.rs | Add UI test for mode switching |
| Two-page layout | Implemented | src/viewer_state.rs, src/window.rs | Improve visual spacing rules |
| Rotation | Implemented | src/navigation.rs, src/viewer_state.rs | Add persistence per document |
| In-page search | Implemented | src/rendering.rs, src/app.rs, src/window.rs | Extend to cross-page search |
| Thumbnails | Partial | src/sidebar.rs (page navigator exists, no real thumbnail strip yet) | Implement real thumbnail rendering cache |
| Outline / bookmarks | Missing | N/A | Implement Poppler outline extraction + sidebar tree |
| Printing | Implemented (core) | src/printing.rs | Add print options UI and compatibility matrix |
| Basic session restore | Implemented | src/session.rs, src/window.rs | Persist per-document view state |

## P1 — Advanced Parity

| Feature | Status | Evidence | Next Action |
|---|---|---|---|
| Fullscreen | Implemented | src/window.rs | Add keybinding regression test |
| Presentation mode | Implemented | src/navigation.rs, src/window.rs | Add navigation behavior contract |
| Document properties | Implemented | src/dialogs.rs, src/app.rs | Add metadata edge-case tests |
| History / recents | Implemented | src/session.rs | Add cap/eviction test coverage |
| Favorites | Implemented | src/session.rs, src/sidebar.rs | Add duplicate handling tests |
| Per-document settings | Partial | rotation/zoom state exists in memory only | Persist and restore per document |
| Configurable shortcuts | Missing | N/A | Add preferences model + shortcut map |
| Annotations parity | Missing | N/A | Evaluate Poppler coverage and scope |
| Forms parity | Missing | N/A | Technical spike for editable forms |

## P2 — Extensions

| Feature | Status | Notes |
|---|---|---|
| SyncTeX | Missing | Planned only after P0/P1 stabilization |
| DjVu/PostScript modules | Missing | Out of core scope for current cycle |
| Format-specific integrations | Missing | Depends on plugin/backends strategy |

## Current Consistency Notes

1. The original plan required a parity traceability matrix; this file provides that missing artifact.
2. Core architecture goals (worker rendering, bounded cache, responsive navigation) are already met.
3. The highest current parity gaps are outlines/bookmarks, real thumbnail pane, and persistent per-document settings.
