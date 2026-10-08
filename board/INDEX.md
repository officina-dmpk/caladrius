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
| T-009 | Oracle for models (closed-form grids) and fitting (R nls / nlsLM references) | oracle | in progress | T-008 |
| T-010 | caladrius-models: one-compartment closed-form models, derivatives, secondary parameters | engine | todo | T-008, T-009 |
| T-011 | caladrius-fit: weighted least squares, Gauss-Newton Levenberg-Hartley, full output set (to split) | engine | todo | T-010 |
| T-012 | Oracle edge cases without public coverage (interior zeros, missing values, IV infusion, Tlag, dose-normalised, AUMC %%extrap) via synthetic PKNCA runs | oracle | done | T-007 |
| T-013 | caladrius-nca: edge parameters (tlag, vss, auciv*, aumcall.dn), PKNCA profile for replaced values | engine | done | T-012 |
| T-014 | Documented-difference marker in oracle/testkit/conformance; D-01 negative concentrations | oracle | done | T-012 |
| T-015 | caladrius-nca quality flags (NCA-LZ-12b) and T-013 review follow-ups | engine | done | T-013 |
| T-016 | specs/nca.md: PKNCA facts from T-012/T-013 (replaced values, IV first segments, AUMC tail, Tlag) | reader | done | T-013 |
