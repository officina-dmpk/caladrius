# Public oracle

Versioned ground truth for the conformance tests (see `AGENTS.md` section 5).

- `data/`: public datasets as CSV with their origin and license in `data/README.md`.
- `scripts/`: R scripts that compute the expected results (PKNCA), with the options written explicitly and the package versions recorded.
- `expected/`: the generated expected results. Regenerate with the script; never edit by hand.

Private references (coursework exports, reference software outputs) never go here; they live in `private/`, which is git-ignored.

## Layout

- `data/*.csv`, `expected/*.csv` and `expected/*.options.json` at the top level: the NCA cases (PKNCA), read by `cargo xtask conformance`.
- `expected/models/`: exact closed-form values of the six one-compartment models (step 3), from `scripts/models_closed_form.R` (256-bit arithmetic with Rmpfr, cross-checked against a matrix exponential and an ODE solver). One case per model and parameter set: `model_<case>.csv` (`subject,parameter,value`, the subject being `t=<time>` or `scalar`) and `.options.json`.
- `expected/fit/` and `data/fit/`: reference weighted least-squares fits (step 3), from `scripts/fit_wls.R` (`stats::nls` and `minpack.lm::nlsLM`, statistics from the formulas of `specs/fit.md`). One case per dataset and weighting: `fit_<dataset>_<weighting>.csv` and `.options.json`.
- Regenerate: `Rscript oracle/scripts/nca_pknca.R`, `Rscript oracle/scripts/models_closed_form.R`, `Rscript oracle/scripts/fit_wls.R`; all three are deterministic and rewrite the files byte for byte.

## Private oracle

Tests against the reference software's exports live behind the Cargo feature `private-oracle` (`cargo test -p caladrius-nca --features private-oracle`) and read `private/exports/<exercise>/` and `private/coursework/`, which are git-ignored. They are skipped with a message when the files are missing. Comparison rule: equality at the precision displayed in the export. Nothing from `private/` is copied into this repository; the code is `crates/caladrius-testkit/src/private.rs` and `crates/caladrius-nca/tests/oracle_private.rs`, the expected file layout is in Q-011 of `board/QUESTIONS.md`. `cargo xtask conformance` prints private counts on the console, and `cargo xtask conformance --private` also writes them (counts only) into `docs/conformance.md`.
