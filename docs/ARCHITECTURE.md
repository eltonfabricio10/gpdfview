# GPDFVIEW — Current Architecture

**Audit snapshot:** 2026-11-03. This describes the code observed in the supplied ZIP, not an idealized target architecture.

## 1. High-level structure

```text
GTK4 / libadwaita application
└── Application + main window
    ├── Shared header/menu actions
    ├── GtkNotebook document tabs
    │   └── ViewerController per opened document
    │       ├── ViewerState (page, zoom, rotation, layout, search)
    │       ├── DrawingArea and per-document toolbar
    │       ├── PdfRenderer worker
    │       │   └── Poppler document + Cairo page surfaces
    │       ├── bounded page LRU cache
    │       └── PrintService worker
    ├── Sidebar (thumbnails, outline, search, recents, favorites, bookmarks)
    └── Session/settings files under the user data directory
```

The diagram is a conceptual map. There are important integration gaps around resolving the active controller for shared window widgets and rebinding sidebar state when the notebook page changes.

## 2. Module responsibilities

| Layer | Modules | Current role |
|---|---|---|
| Application lifecycle | `main.rs`, `app.rs` | Initialize GTK/libadwaita application, build UI, own controller state and application actions |
| Window and widgets | `window.rs`, `ui_helpers.rs`, `dialogs.rs` | Build window, toolbar, page drawing area, menus, settings/properties dialogs |
| Document/tab workflows | `document_actions.rs`, `document_tabs.rs` | Select files, create document controllers, manage notebook pages and recently closed metadata |
| Viewer model | `viewer_state.rs`, `navigation.rs`, `layout.rs` | Track page, scale, rotation, layout, split state and calculate geometry/navigation |
| PDF engine | `engine.rs`, `poppler_lock.rs` | Load Poppler documents, render pages/thumbnails, extract outline and search text; serialize some native access via a mutex |
| Render presentation | `rendering.rs`, `cache.rs` | Consume render/search results and maintain bounded cached page surfaces |
| Sidebar | `sidebar.rs` | Build thumbnail/outline/search/recent/favorite/bookmark panels |
| Commands and input | `ui_commands.rs`, `keyboard.rs` | Map UI and keyboard gestures to viewer actions |
| Print pipeline | `printing.rs` | Background print queue, CUPS `lp` submission and Cairo PDF fallback |
| Persistence | `session.rs` | Text-file settings, recent items, favorites, app bookmarks, window state and per-document viewer state |

## 3. Document lifecycle

1. `GApplication` receives activation or open requests in `app.rs`.
2. The file-picker path in `document_actions.rs` can select multiple PDFs and create a new tab per path.
3. Opening a tab creates a `ViewerController` and calls `new_worker_pair()` to create a renderer and print service.
4. The controller owns the document's viewer state, drawing area, page cache, UI controls and worker handles.
5. Page changes enqueue a render request; the renderer worker uses Poppler and returns pixel/surface data for presentation by GTK.
6. Closing a tab removes its notebook page and parallel title/path entries; worker lifetime/shutdown behavior should be tested.

### Integration issue

Shared window widgets and menus are not consistently routed through the selected notebook page. Several callbacks capture the initial controller, the sidebar is also tied to that controller, and the shared print button receives a callback each time a controller is created. A central `ActiveDocument`/controller registry is needed before reliable multi-tab behavior can be claimed.

## 4. Rendering, cache and threads

- `PdfRenderer` creates one named worker thread for each renderer instance.
- The worker has a pending visible request and a prefetch queue. A newer visible request can supersede old pending visible work; a generation value helps reject obsolete results.
- Each viewer has a page cache limited to 128 MiB (`app.rs`).
- The sidebar has a separate thumbnail cache limited to 24 MiB (`sidebar.rs`).
- The current thumbnail builder focuses on a small page window around the current page rather than materializing every thumbnail in a very large file.
- `new_worker_pair()` creates both a renderer worker and a print worker for each opened document. Worker count therefore grows with tab count.
- `poppler_lock.rs` provides a global mutex, but source inspection found Poppler operations that do not consistently acquire it. The code should define whether documents are worker-owned, guarded by the global mutex, or isolated per task.

### Target architecture

Keep GTK widgets and all GTK mutations on the main thread. Pass immutable task parameters to workers and return plain Rust data (IDs, page indices, pixel buffers, error values) through channels. If Poppler objects cannot safely be shared between threads, open/use them within a single worker or guard every native operation consistently. Add task IDs, cancellation/generation checks and explicit worker shutdown.

Do not assume `Send`/`Sync` for Poppler/GObject values. Verify the guarantees of the exact binding/native library version before changing thread topology.

## 5. Search and sidebar data flow

Current document-wide search calls `find_text_rectangles_in_document`, which loops through pages and searches each page. The search action invokes this path synchronously. Thumbnail and outline content are also generated/populated synchronously from sidebar construction or refresh.

Target flow:

```text
UI action
  → enqueue task { document_id, query/page range, generation }
  → worker performs Poppler operation
  → send incremental result messages
  → main thread checks document_id + generation
  → update state/widgets and draw
```

This avoids blocking the GTK main loop and prevents stale results from a closed tab or superseded query from updating the visible document.

## 6. Printing data flow

The current print service uses a bounded request channel and a worker thread. The normal path invokes `lp` with PDF format and options. If submission fails, a fallback opens the source through Poppler, renders selected pages to a temporary Cairo PDF, then submits that generated PDF.

The fallback currently applies scale while generating the temporary PDF and forwards the same scale to CUPS. Confirm whether backend scaling applies again and ensure there is exactly one scaling stage. Printer destination and media capabilities are not yet fully integrated.

## 7. Persistence

`session.rs` stores configuration and viewer data as plain text files in the gpdfview user-data directory. It stores recent documents, favorites, page bookmarks, window state and per-document page/scale/rotation/layout.

Current limitations:

- writes frequently ignore errors;
- writes are not consistently atomic;
- the format has no explicit schema version/migration;
- startup restore is not a complete record of all open tabs;
- path identity alone may not detect replaced/moved files.

A future storage layer should provide versioned schema, atomic replacement, validated values and observable errors. SQLite is optional; a small versioned file format may be sufficient.

## 8. Architectural decisions to settle

1. **Active document:** one controller registry keyed by stable document ID, with all shared actions resolving the active ID.
2. **Tab metadata:** one stable object per tab, not parallel arrays indexed by notebook position.
3. **Poppler ownership:** worker-owned documents or one consistently guarded access layer; document the invariant.
4. **Worker topology:** per-document dedicated worker vs bounded global pool; decide based on benchmarks and Poppler safety.
5. **Search model:** asynchronous incremental search with cancellation and stale-result suppression.
6. **Sidebar model:** virtualized, demand-loaded thumbnails and asynchronous outline parsing.
7. **Printing:** separate PDF submission from render fallback options so scale is applied exactly once.
8. **Persistence:** versioned, atomic and diagnosable.

See [`PROJECT_AUDIT.md`](PROJECT_AUDIT.md) for findings and [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) for implementation order.
