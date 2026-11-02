# New Feature Roadmap (Post-Alignment)

Date: 2026-11-02

This roadmap prioritizes highest-value parity gaps while preserving the current stable architecture.

## Guiding Rules

- Deliver in small vertical slices with end-to-end validation.
- Keep GTK main thread free from heavy operations.
- Add tests with each feature increment (unit + integration where practical).
- Prefer architecture reuse: engine + rendering + sidebar + state modules.

## Sprint A — Outline/Bookmarks Sidebar (High Priority)

Goal: add a document outline tree for fast structural navigation.

Scope:
- Extract outline/bookmarks from Poppler in engine layer.
- Add sidebar section for outline tree.
- Navigate to target page when selecting an outline node.
- Handle documents with no outline gracefully.

Acceptance criteria:
- Outline is displayed when available.
- Clicking item navigates correctly.
- UI remains responsive while loading outline.
- Tests validate parse and navigation behavior.

## Sprint B — Real Thumbnail Pane (High Priority)

Goal: evolve page navigator into real visual thumbnails.

Scope:
- Add thumbnail render path at reduced scale.
- Create dedicated thumbnail cache (separate from main-page cache).
- Lazy-load thumbnails for visible sidebar rows.
- Preserve quick navigation to clicked page.

Acceptance criteria:
- Thumbnail pane renders progressively.
- Scrolling sidebar does not freeze main view.
- Memory use remains bounded with explicit limits.
- Tests cover cache eviction and request coalescing.

## Sprint C — Cross-Page Search UX (Medium Priority)

Goal: upgrade search from current-page only to document-level traversal.

Scope:
- Add “find next in document” and “find previous in document”.
- Maintain current match context across pages/tabs.
- Show match counters (example: 14/57).
- Optional incremental search debounce.

Acceptance criteria:
- Next/previous traverses pages when needed.
- Status and highlight remain in sync.
- No stale highlight artifacts after page changes.
- Tests cover wrap-around behavior and tab isolation.

## Sprint D — Per-Document Persistent View State (Medium Priority)

Goal: persist user context per document across sessions.

Scope:
- Persist page index, zoom, rotation, layout mode by document path/fingerprint.
- Restore state on reopen.
- Invalidate gracefully when document changes externally.

Acceptance criteria:
- Reopen resumes last view state.
- Missing/changed files do not break startup.
- Tests validate save/load and corruption resilience.

## Sprint E — Print Options Expansion (Medium Priority)

Goal: improve printing parity and observability.

Scope:
- Add page range, copies, orientation and scale options.
- Keep direct PDF path preferred; fallback to rendered path.
- Record print path chosen and elapsed time.

Acceptance criteria:
- User options are applied to print job.
- Fallback path works for unsupported direct mode.
- Logs/metrics identify selected print route.
- Tests validate option mapping.

## Planned Sequence

1. Sprint A (outline/bookmarks)
2. Sprint B (thumbnails)
3. Sprint C (cross-page search)
4. Sprint D (per-document persistence)
5. Sprint E (printing options)

## Execution Checklist Template (per sprint)

- Define API boundaries (engine/app/sidebar/state)
- Implement feature in smallest vertical path
- Add/adjust tests
- Run cargo test -q
- Smoke-check manual flow
- Update PARITY_MATRIX.md and README
