# Board index

One line per task: id, title, role, state. Only the orchestrator writes this file. Cards live in `tasks/`.

| id | title | role | state | depends on |
|---|---|---|---|---|
| T-000 | Step 0: contract, roles, board, repository | orchestrator | done | |
| T-001 | Cargo workspace skeleton, xtask layers/wasm, lints | interface | done | T-000 |
| T-002 | specs/nca.md from allowed sources (λz, AUC rules, BLQ, extrapolation) | reader | done | T-000 |
| T-003 | Public NCA oracle: theophylline and indomethacin with PKNCA, testkit tolerances, failing tests | oracle | done | T-001 |
| T-004 | caladrius-nca (umbrella, split into T-004a/b/c) | engine | split | T-002, T-003 |
| T-004a | caladrius-nca: input types, validation errors, AUC rules (linear, lin-up/log-down), Cmax/Tmax/Tlast/Clast | engine | done | T-002, T-003 |
| T-004b | caladrius-nca: λz selection, t½, AUC/AUMC extrapolation, %extrapolated | engine | done | T-004a |
| T-004c | caladrius-nca: C0 back-extrapolation, AUMC, MRT, CL/F, Vz/F, Vss; 100% public oracle; conformance table | engine | done | T-004b |
| T-005 | Synthetic oracle profiles to settle λz open items O-01 and O-02 (PKNCA runs) | oracle | done | T-003 |
| T-006 | xtask conformance hardening: per-parameter floors, parse errors, computed-presence check, atomic write | interface | done | T-004c |
| T-007 | specs/nca.md sync: oracle-confirmed tags (O-01, O-02, LZ-05 strict >, LZ-07), dose NC instead of error (DAT-11, 2.4, section 9) | reader | done | T-004c, T-005 |
| T-008 | specs/models.md and specs/fit.md | reader | done | T-007 |
| T-009 | Oracle for models (closed-form grids) and fitting (R nls / nlsLM references) | oracle | done | T-008 |
| T-010 | caladrius-models: one-compartment closed-form models, derivatives, secondary parameters | engine | done | T-008, T-009 |
| T-011 | caladrius-fit (umbrella, split into T-011a/b) | engine | split | T-010 |
| T-011a | caladrius-fit core: WLS, Gauss-Newton Levenberg-Hartley, estimates, SE, CV%, WRSS, AIC/SBC | engine | done | T-010 |
| T-011b | caladrius-fit outputs: CI, matrices, condition numbers, trace, initial estimates, bounds | engine | done | T-011a |
| T-012 | Oracle edge cases without public coverage (interior zeros, missing values, IV infusion, Tlag, dose-normalised, AUMC %%extrap) via synthetic PKNCA runs | oracle | done | T-007 |
| T-013 | caladrius-nca: edge parameters (tlag, vss, auciv*, aumcall.dn), PKNCA profile for replaced values | engine | done | T-012 |
| T-014 | Documented-difference marker in oracle/testkit/conformance; D-01 negative concentrations | oracle | done | T-012 |
| T-015 | caladrius-nca quality flags (NCA-LZ-12b) and T-013 review follow-ups | engine | done | T-013 |
| T-016 | specs/nca.md: PKNCA facts from T-012/T-013 (replaced values, IV first segments, AUMC tail, Tlag) | reader | done | T-013 |
| T-017 | specs/models.md and specs/fit.md sync after the oracle (M5 fix, precision note, fit facts, tags) | reader | done | T-009, T-010, T-011 |
| T-018 | Arbitration of the 1/ŷ fit oracle cases (fixed points) and the F1 trace tolerance | oracle | done | T-009, T-011a |
| T-019 | Private oracle infrastructure (feature private-oracle, export loader, private conformance counts) and exercise 1 NCA | oracle | done | T-013 |
| T-020 | caladrius-project (data model) and caladrius-engine (command registry, schemas, history) | interface | done | T-011b |
| T-021 | caladrius-cli and conformance extended to models and fit | interface | done | T-020 |
| T-022 | caladrius-mcp (MCP server over stdio) | interface | done | T-020 |
| T-023 | specs/nca.md: facts observed on the reference screen (default AUC method, λz weighting, empty acceptance criteria) | reader | done | T-016 |
| T-024 | caladrius-ui first slice: theme tokens, worksheet, NCA page with λz point selection, snapshots | interface | done | T-022, T-017 |
| T-025 | Units in NCA results (time/conc/dose units as data, derived units per parameter) | engine | done | T-015 |
| T-026 | caladrius-ui second slice: model fit page with live curve, diagnostics plots, simulation page | interface | done | T-024 |
| T-027 | caladrius-ui third slice: project save/load, command palette, settings page, T-026 follow-ups | interface | done | T-026 |
| T-028 | Release scaffolding for v0.1.0: GitHub Actions CI, CITATION.cff, conformance wording fix | interface | done | T-027 |
| T-029 | Honest validation claims: coverage table per model, tolerance note, missing fit oracles (infusion, zero-order, lag) | oracle | done | T-021 |
| T-030 | Closed-form derivatives for infusion, zero-order and lag models; xtask fit conformance with fixed parameters | engine | done | T-029 |
| T-031 | specs/models.md: two-compartment models (parameterisations, closed forms, derivatives, ids, oracle cases) | reader | done | T-017 |
| T-032 | Oracle for the two-compartment models (grids, 256-bit, deSolve, derivatives, failing tests) | oracle | done | T-031 |
| T-033 | caladrius-models pk2.* (two compartments), derivatives, xtask conformance for pk2 | engine | done | T-032 |
| T-034 | pk2 reachable from the engine, CLI and MCP (a, now), then the UI (b, after the benchmark) | interface | done | T-033 |
| T-035 | specs sync after T-032/T-033: output names, AUMC(0,t), dose 0 conventions, OF-08 (no automatic initial estimates for pk2), confirmed-by-oracle tags for MOD-2C | reader | done | T-033 |
| T-036 | cargo xtask lint: forbidden names, private/ leaks, deny-lints header, tolerances once, board hygiene; CI step | interface | done | T-028 |
| T-037 | README: sync the verified section with main (PR #1, external contribution) | orchestrator | done | T-034 |
| T-038 | docs/screens: the application illustrated from the snapshot example (PR #2, external contribution) | orchestrator | done | T-034 |
| T-040 | Default fit settings reach the exact minimum (Q-014 option 2); reference_conventions preset; D-04 | engine | todo | T-030 |
