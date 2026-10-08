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
| T-004b | caladrius-nca: λz selection, t½, AUC/AUMC extrapolation, %extrapolated | engine | in progress | T-004a |
| T-004c | caladrius-nca: C0 back-extrapolation, AUMC, MRT, CL/F, Vz/F, Vss; 100% public oracle; conformance table | engine | todo | T-004b |
| T-005 | Synthetic oracle profiles to settle λz open items O-01 and O-02 (PKNCA runs) | oracle | done | T-003 |
