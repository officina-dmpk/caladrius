# Public oracle

Versioned ground truth for the conformance tests (see `AGENTS.md` section 5).

- `data/`: public datasets as CSV with their origin and license in `data/README.md`.
- `scripts/`: R scripts that compute the expected results (PKNCA), with the options written explicitly and the package versions recorded.
- `expected/`: the generated expected results. Regenerate with the script; never edit by hand.

Private references (coursework exports, reference software outputs) never go here; they live in `private/`, which is git-ignored.

## Layout

- `data/*.csv`, `expected/*.csv` and `expected/*.options.json` at the top level: the NCA cases (PKNCA), read by `cargo xtask conformance`.
- `expected/models/`: exact closed-form values of the six one-compartment models (step 3), from `scripts/models_closed_form.R` (256-bit arithmetic with Rmpfr, cross-checked against a matrix exponential and an ODE solver). One case per model and parameter set: `model_<case>.csv` (`subject,parameter,value`, the subject being `t=<time>` or `scalar`) and `.options.json`.
- `expected/models/pk2/`: the two-compartment models (task T-032, `specs/models.md` section 11), from `scripts/models_2c_closed_form.R` and `scripts/models_2c_derivatives.R` (sourced by the first). Same long format and options files as the one-compartment cases, in a subdirectory so that the one-compartment loaders and `cargo xtask conformance` do not see them until the engine and interface cards for two compartments land (loaders: `caladrius_testkit::pk2`). Three kinds of cases: `model_pk2_<id>_<case>` (`conc`, `auc`, `aumc` on a time grid and the scalars: the three parameter sets, half-lives, volumes, areas to infinity, MRT, Tmax and Cmax), `model_pk2_deriv_<id>_<case>` (partial derivatives of the concentration with respect to the parameters of the set the case is given in and to `ka`, `tlag`, `dur`, from 256-bit central differences; entries with a condition number above 1e3 are omitted, see the options) and `model_pk2_errors` (what the engine must refuse, as not-available rows with the reason and the words of the message). The concentrations are independent explicit sums of exponentials in 256-bit arithmetic (Rmpfr), cross-checked on every case against a matrix exponential (expm) and an ODE solver (deSolve lsoda). The script also writes `crates/caladrius-models/tests/oracle_models_2c.cases`, the list of cases the engine test runs.
- `expected/fit/` and `data/fit/`: reference weighted least-squares fits (step 3), from `scripts/fit_wls.R` (`stats::nls` and `minpack.lm::nlsLM`, statistics from the formulas of `specs/fit.md`). One case per dataset and weighting: `fit_<dataset>_<weighting>.csv` and `.options.json`. Datasets: `theoph`, `indometh`, `spec` (the five-point example), and, since task T-029, the synthetic `infusion`, `zero_order` and `lag` profiles of `data/fit/synthetic_*.csv` (written by the same script, fixed seed, for `pk1.iv_infusion`, `pk1.oral_0` and `pk1.oral_1_lag`; the options file has the fixed parameters in `fixed` and the generation in `generated_data`).
- Regenerate: `Rscript oracle/scripts/nca_pknca.R`, `Rscript oracle/scripts/models_closed_form.R`, `Rscript oracle/scripts/models_2c_closed_form.R` (about 30 minutes: 256-bit central differences), `Rscript oracle/scripts/fit_wls.R`; all four are deterministic and rewrite the files byte for byte.

## Private oracle

Tests against the reference software's exports live behind the Cargo feature `private-oracle` (`cargo test -p caladrius-nca --features private-oracle`) and read `private/exports/<exercise>/` and `private/coursework/`, which are git-ignored. They are skipped with a message when the files are missing. Comparison rule: equality at the precision displayed in the export. Nothing from `private/` is copied into this repository; the code is `crates/caladrius-testkit/src/private.rs` and `crates/caladrius-nca/tests/oracle_private.rs`, the expected file layout is in Q-011 of `board/QUESTIONS.md`. `cargo xtask conformance` prints private counts on the console, and `cargo xtask conformance --private` also writes them (counts only) into `docs/conformance.md`.
