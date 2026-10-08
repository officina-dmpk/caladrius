# Questions for the human

Each entry: the question, the decision taken meanwhile, and the human's answer when given.

## Q-001 (2026-10-08, orchestrator): changes made to the brief when creating `AGENTS.md`

Decisions taken meanwhile, to confirm or revert:

1. Project name Caladrius, crates prefixed `caladrius-`, the bare `caladrius` crate reserved for the desktop app (free on crates.io and PyPI on 2026-10-08).
2. `apps/caladrius-mcp` added to the v1 scope and to step 4 of the marching order, so that Apothicaire can call the command registry.
3. Golden rule 9 (wasm): advisory until step 6, a gate from step 6 on; platform-only code gated behind `cfg` and kept in `apps/`.
4. A `janitor` role (Haiku) for mechanical work.
5. Repository at `C:\Users\abdou\apothicaire\caladrius`, its own git repository, separate from the Apothicaire agent; can be moved out later without changes.

Answer (2026-10-08, human): all five confirmed.

## Q-002 (2026-10-08, orchestrator): reference software license

Before any comparison with the reference software's outputs in `private/`, the human must check the license granted by the university: is comparing results and publishing a conformance table allowed? Until answered, the private oracle feature stays off and no number from `private/` is produced by an agent.

Answer (2026-10-08, human): yes, comparing is allowed. The private oracle feature may be used once files are dropped in `private/`. Publication of any numeric comparison still requires the human's explicit decision (section 2.4 of `AGENTS.md`).

## Q-003 (2026-10-08, orchestrator): R packages

T-003 needs `PKNCA` installed in R (`install.packages("PKNCA")`). `AGENTS.md` allows installs through package managers, so the oracle agent will do it unless told otherwise.

Answer (2026-10-08, human): ok, install PKNCA.

## Q-004 (2026-10-08, oracle): license of the public datasets and of PKNCA output

T-003 adds `Theoph` and `Indometh` (R `datasets` package, distributed with R under GPL-2 or GPL-3) as CSV files in `oracle/data/`, and expected values computed with PKNCA (AGPL-3). The numbers are experimental measurements and computed results, but the CSV files are derived from files distributed under a copyleft license, while Caladrius is MIT OR Apache-2.0.

Decision taken meanwhile: keep the data and expected values in `oracle/`, list them in `ATTRIBUTION.md` with their origin and license, copy no code or documentation text from R or PKNCA, and keep `oracle/` out of any published crate (it is not under `crates/`). To confirm before the first release: either keep as is, or replace the CSV files by a pointer plus a script that regenerates them from R at test time.

Orchestrator note (2026-10-08): decision stands. Measured concentrations are facts, not code; R's GPL covers the package, PKNCA's AGPL covers its code, neither covers computed output. `oracle/` is not part of any published crate. To confirm by the human before the first public release.

Answer (human): pending.
