# Specifications

Behaviour and UX specifications written by the `reader` agent in its own words and formulas (see `AGENTS.md` section 6). Implementer agents read these files and the oracle, nothing else.

Every rule carries a status tag:

- `confirmed by oracle`: a versioned test reproduces it.
- `documented, untested`: found in an allowed source, not yet tested.
- `assumed`: our best guess; becomes a test task before being relied on.
- `observed`: seen on the human's screen of the reference software (an allowed source), not yet tested against a private export.
- `later`: a model or convention that an allowed source describes but that lies outside the current marching order (`AGENTS.md` section 12); listed with its source pages, not specified, no test; not counted in the status tables.

Files: `nca.md` (T-002), `models.md` (T-008; one compartment, sections 1 to 10; two compartments, section 11, T-031: `MOD-2C-*`; synced with the two-compartment oracle and engine by T-035, where 18 of its 25 rules are `confirmed by oracle`; T-046 added page citations from the paper S-36 (Bertrand and Mentré 2008) as a second source to the rules it covers, section 12 on multiple dosing and steady state, `MOD-MD-*`, 12 rules `documented, untested` or `assumed` awaiting the oracle OM-16, and section 13, `MOD-LT-*`, three compartments, Michaelis-Menten elimination and the effect compartment, status `later`), `fit.md`, `ux.md`, `differences.md` (documented method differences with the reference software), `sources.md` (sources consulted and their terms of use).
