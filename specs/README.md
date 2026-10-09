# Specifications

Behaviour and UX specifications written by the `reader` agent in its own words and formulas (see `AGENTS.md` section 6). Implementer agents read these files and the oracle, nothing else.

Every rule carries a status tag:

- `confirmed by oracle`: a versioned test reproduces it.
- `documented, untested`: found in an allowed source, not yet tested.
- `assumed`: our best guess; becomes a test task before being relied on.
- `observed`: seen on the human's screen of the reference software (an allowed source), not yet tested against a private export.

Files: `nca.md` (T-002), `models.md` (T-008; one compartment, sections 1 to 10; two compartments, section 11, T-031: `MOD-2C-*`; synced with the two-compartment oracle and engine by T-035, where 18 of its 25 rules are `confirmed by oracle`), `fit.md`, `ux.md`, `differences.md` (documented method differences with the reference software), `sources.md` (sources consulted and their terms of use).
