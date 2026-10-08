# Log (append only, one entry per finished task)

- 2026-10-08 T-000 (orchestrator): contract `AGENTS.md` adapted from the human's brief (name Caladrius, crate prefix `caladrius-`, MCP server in v1, wasm advisory until step 6, janitor role), agent definitions in `.claude/agents/`, board created with T-001 to T-004, repository initialised.
- 2026-10-08 T-001 (interface): 11-crate workspace, deny lints, xtask layers/wasm (30 tests), wasm target installed; commit c8b7e7c. Orchestrator added clippy.toml, licenses, .gitignore fix.
- 2026-10-08 T-003 (oracle): Theoph and Indometh datasets, PKNCA 0.12.1 script, 1080 expected values in 4 cases, testkit (Tolerance, table comparison, loaders, 36 tests), 20 failing oracle tests against the assumed `caladrius_nca::run` API; commit 4b670e4.
