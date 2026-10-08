# Log (append only, one entry per finished task)

- 2026-10-08 T-000 (orchestrator): contract `AGENTS.md` adapted from the human's brief (name Caladrius, crate prefix `caladrius-`, MCP server in v1, wasm advisory until step 6, janitor role), agent definitions in `.claude/agents/`, board created with T-001 to T-004, repository initialised.
- 2026-10-08 T-001 (interface): 11-crate workspace, deny lints, xtask layers/wasm (30 tests), wasm target installed; commit c8b7e7c. Orchestrator added clippy.toml, licenses, .gitignore fix.
- 2026-10-08 T-003 (oracle): Theoph and Indometh datasets, PKNCA 0.12.1 script, 1080 expected values in 4 cases, testkit (Tolerance, table comparison, loaders, 36 tests), 20 failing oracle tests against the assumed `caladrius_nca::run` API; commit 4b670e4.
- 2026-10-08 T-002 (reader): specs/nca.md (61 rules: 50 documented, 11 assumed; 42-row vocabulary; 8 worked examples; 15 open items) and specs/sources.md; reference documentation NOT used (terms of use), Q-005..Q-008 raised.
- 2026-10-08 T-004a (engine): caladrius-nca types, validation (16 tests), cleaning, AUC/AUMC rules (linear, lin-up/log-down, lin-log), observed parameters; 9/21 oracle tests green; review: 7 findings fixed; commits ac432b2, 54ce34e.
- 2026-10-08 T-005 (oracle): synthetic_lz profiles (D1, D2, searched subjects) settle O-01 (tolerance reading, strict >, most points) and O-02 (positive-slope filter after selection); 2 new oracle cases x 116 values, 16 oracle_synthetic tests, 6 testkit discrimination tests.
- 2026-10-08 T-004b (engine): lambda_z.rs and extrapolation.rs; terminal_phase and extrapolation green on 6 oracle cases (17/21 public, 14/16 synthetic); new NcaOptions::lambda_z_selection; commit 48feaed; review approved.
- 2026-10-08 T-004c (engine): derived parameters (MRT, CL/F, Vz/F, Vss, .dn), NCA-DAT-10, T-004b follow-ups, `cargo xtask conformance` (docs/conformance.md); 21/21 + 16/16 oracle tests, 1312/1312 values (100%); commit b76dc8b; review approved. STEP 2 REACHED.
- 2026-10-08 T-006 (interface): conformance gate hardened (per-parameter floors, parse errors, computed-presence rule, atomic write, 41 xtask tests); commit 1d82059.
- 2026-10-08 T-007 (reader): specs/nca.md synced with the oracle and engine: 66 rules (30 confirmed by oracle), LZ-05 strict >, dose NC reasons, O-01/O-02/O-08 closed, O-16/O-17 opened.
- 2026-10-08 T-008 (reader): specs/models.md (24 rules, M1-M5) and specs/fit.md (39 rules, F1-F4); Q-009 raised.
- 2026-10-08 T-012 (oracle): 14 edge cases, 2448 expected values, 88 engine tests (62 pass), 6 testkit cross-checks; conformance 3522/3760 (public 100%); decisions on replaced values and negatives -> T-013, T-014.
- 2026-10-08 T-014 (oracle): documented-difference marker (options.json, testkit, conformance column), specs/differences.md D-01 (trailing negative never Clast); commit 18c6d73.
- 2026-10-08 T-013 (engine): tlag, vss.obs/pred, vss.iv.last, aumcall.dn, auciv* family, PKNCA profile for replaced values (+ exclude_replaced option); 88/88 edge tests; conformance 3744/3744 + 16 documented; commit 46c0ce1; review approved.
- 2026-10-08 T-015 (engine): quality flags (6 kinds, thresholds in NcaOptions::quality), no_positive_concentration rule, tlag reasons, exclude_replaced tests; 10 flag tests; commit 2b32db6; review approved.
