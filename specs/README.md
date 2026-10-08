# Specifications

Behaviour and UX specifications written by the `reader` agent in its own words and formulas (see `AGENTS.md` section 6). Implementer agents read these files and the oracle, nothing else.

Every rule carries a status tag:

- `confirmed by oracle`: a versioned test reproduces it.
- `documented, untested`: found in an allowed source, not yet tested.
- `assumed`: our best guess; becomes a test task before being relied on.

Files: `nca.md` (T-002), later `models.md`, `fit.md`, `ux.md`, `differences.md` (documented method differences with the reference software), `sources.md` (sources consulted and their terms of use).
