# gpdfview Feature Parity Matrix

**Current matrix:** [`docs/FEATURE_PARITY.md`](docs/FEATURE_PARITY.md)  
**Technical findings:** [`docs/PROJECT_AUDIT.md`](docs/PROJECT_AUDIT.md)  
**Implementation plan:** [`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md)

This root-level file is retained for discoverability. The older matrix content was based on a prior source snapshot and incorrectly marked several existing code paths (notably thumbnails, outline extraction, document-wide search and per-document state persistence) as absent.

Use the detailed matrix in `docs/FEATURE_PARITY.md` as the source of truth. Its statuses distinguish `IMPLEMENTADO`, `PARCIAL`, `AUSENTE`, `COM PROBLEMAS` and `NÃO APLICÁVEL`; code presence is not treated as proof of runtime or performance parity.
