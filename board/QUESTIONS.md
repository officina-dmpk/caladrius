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

## Q-005 (2026-10-08, reader): terms of the reference software's public documentation

`AGENTS.md` section 6 asks the reader to check the documentation site's terms before using it. Result (details in `specs/sources.md` section 2): the legal notice shipped with the documentation says it is for use by the vendor's customers under the controlling licence agreement, that any other use is not authorised, and that no part may be reproduced, transmitted or translated without the licence agreement's permission or the vendor's written consent. The vendor website's own notice also forbids automated access and any other use of its material. Neither text gives permission for what this project would do (an automated agent reading the pages to write an independent open-source specification), and the licence agreement that applies (your university licence, see Q-002) is not visible to the reader. Read as written, the use is not allowed.

Decision taken meanwhile: the reader stopped and did not use the reference documentation. `specs/nca.md` rests on the PKNCA documentation, peer-reviewed papers, a conference poster, and the public documentation of other NCA tools. Every convention of the reference software that these sources do not settle is tagged `assumed` and becomes a test for the private oracle (observed results), not a documented fact. Question Q-008 asks you to read a few default settings off your own screen.

Disclosure: three web searches the reader ran to find the site returned tool-written summaries that quoted or paraphrased pages of the reference documentation and of forum threads about it (topics listed in `specs/sources.md` section 2.4). The reader did not open those pages and no rule depends on them, but the orchestrator may want to judge whether this matters for the clean-room record.

What you can do: (a) check your university licence and the full text of the two notices, and tell the orchestrator if you authorise the reader to read the documentation; (b) read the pages yourself and answer targeted questions; or (c) do nothing, in which case the private exports settle the conventions.

Orchestrator decision (2026-10-08): the reader's decision stands; the reference documentation is not used. The incidental search summaries are disclosed and no rule depends on them: acceptable for the clean-room record. Conventions specific to the reference software are settled by observed results (private oracle, Q-008), never by its documentation.

Answer: _pending_.

## Q-006 (2026-10-08, reader): vendor and product names in `specs/sources.md`

`AGENTS.md` keeps "Phoenix", "WinNonlin" and "Certara" out of the docs except in a neutral compatibility statement and the trademark notice. `specs/sources.md` has to record the address of the documentation site it checked (a URL containing the vendor's name) for the terms-of-use record; `specs/nca.md` uses only "the reference software".

Decision taken meanwhile: the vendor name appears in `specs/sources.md` only, inside the URLs of the terms check, as a provenance record. Strike or move it if you prefer.

Orchestrator decision (2026-10-08): accepted, vendor name only inside the URLs of the terms-of-use record in `specs/sources.md`.

Answer: _pending_.

## Q-007 (2026-10-08, reader): published reference-software numbers

An open-access paper (Kim et al., Transl Clin Pharmacol 2018, PMC6989226, licence CC BY-NC 3.0) prints a table comparing an R package with the reference software on one subject of the public theophylline data. The reader reproduced those printed values with the rules of `specs/nca.md` (hand check H3 in `specs/sources.md`). Two points need your decision, since `AGENTS.md` makes any published numeric comparison with the reference software yours: (a) may the statement "the rules reproduce the published table" stay in versioned files (`specs/sources.md` H3 and item O-14 of `specs/nca.md`) before you decide on publication? (b) the table's licence is non-commercial, which does not fit MIT OR Apache-2.0, so its numbers are not copied into `oracle/`; do you accept that, or want them used as a public test after checking the licence question?

Decision taken meanwhile: no number from that paper is in any versioned file; the agreement statement stays; strike it if you prefer.

Orchestrator decision (2026-10-08): accepted. No number from CC BY-NC material enters a versioned file; the agreement statement may stay. Publication of comparisons remains the human's decision.

Answer: _pending_.

## Q-008 (2026-10-08, reader): defaults to read off your own screen

Observed results are an allowed source and cost no licence risk. Please look at the NCA setup of the reference software on your machine (do not open its install folder) and tell the orchestrator, in your words:

1. the default AUC calculation method, and the list of methods it offers;
2. the default rule for values below the limit of quantification, and the rules it offers;
3. the default λz method, whether the point at Cmax or Tmax is left out of the automatic choice (for oral data and for IV bolus data), and any minimum adjusted R², span or similar acceptance setting;
4. whether, for IV bolus data without a sample at time 0, the extrapolated C0 is added to the profile before the AUC is computed;
5. what Tlag is for a profile that starts with zeros.

Observed by the human on his screen (2026-10-08, NCA setup, reported by the orchestrator in its own words; screenshots kept in `private/screenshots/` for the reader only):

1. AUC method: four choices: linear trapezoid with linear interpolation (the default), linear-log trapezoid, linear-up/log-down, and linear trapezoid with linear/log interpolation.
2. BLQ: a separate "BQL rules" object exists in the project tree; not yet observed.
3. λz: the regression has its own weighting (uniform by default; 1/Y, 1/Y², user-defined also offered). The best-fit rules are a maximum number of points and an earliest start time, both empty by default. Acceptance criteria (adjusted R² ≥, %AUC extrapolated ≤, span ≥, number of samples ≥, %AUC_tau extrapolated ≤) exist and are all empty by default: no acceptance threshold is enforced unless the user sets one. A "disable curve stripping" setting exists. Exclusion of Cmax: not yet observed.
4. C0 for IV bolus: not yet observed. Model types: plasma (200-202), urine (210-212), drug effect (220). Dose type default: extravascular. A dose normalisation setting exists (none by default). For steady state (non-bolus) an imputed concentration at dose time can be Cmin, Ctau or Clast.
5. Tlag: not yet observed.

Further observation (2026-10-08): the preferences hold no global analysis defaults (the object-settings page only stores user-saved profiles; plotting defaults are sizes and margins), so defaults are those of each analysis object. The worksheet grid follows the Windows regional decimal symbol; with a French locale a decimal point is refused and a CSV with decimal commas is split on the comma (UX friction 10).

Decision taken meanwhile: the public oracle uses PKNCA's defaults; the "reference profile" of `specs/nca.md` stays `assumed`.

Answer: _pending_.

## Q-009 (2026-10-08, reader): fitting defaults and output definitions to read off your own screen

For `specs/fit.md` and `specs/models.md` (card T-008). Same rules as Q-008: look at the setup and the result tables of the reference software on your machine (not its install folder) and tell the orchestrator in your words; numbers from `private/` stay out of versioned files.

1. Weighting: the list of weighting choices it offers and the default.
2. Parameter bounds: when it generates them, what rule it seems to follow (for example a fraction or multiple of the initial estimate, or zero and infinity).
3. Confidence intervals: the two kinds offered next to the final parameters (called univariate and planar in our notes), and the confidence level.
4. In the diagnostics table: whether AIC and SBC are printed with a negative value for a good fit, and which statistic (if any) is printed as "correlation between observed and predicted".
5. For the eigenvalues and condition numbers: whether they are listed for the correlation matrix of the estimates or for another matrix.
6. The model list for one compartment: the names of the parameters printed in the results (names only), and whether a lag time and a zero-order input exist as separate models.

Decision taken meanwhile: `specs/fit.md` and `specs/models.md` follow open-tool conventions and the defaults of `AGENTS.md` section 6; every point above is tagged `assumed` there (open items OF-01 to OF-07, OM-02, OM-03).

Answer: _pending_.

## Q-010 (2026-10-08, reader): replaced values and the extrapolation to infinity

No action needed from you yet; this is a decision record for the orchestrator and a possible later look at your screen. PKNCA, and therefore the engine by default, adds the tail Clast/λz from the last measured positive point after an area that already runs to a later replaced value (a trailing BLQ value replaced by a number), so that stretch is counted twice in AUCinf and AUMCinf (`specs/nca.md` NCA-EXT-01b, worked example W9). The engine reports it with the quality flag `area_past_tlast`.

If you ever use a BLQ rule that replaces trailing values by a number in the reference software, please note whether its AUCinf, observed, equals "area to the last measured positive point plus Clast/λz" or "area to the last replaced point plus Clast/λz" (one profile is enough), and tell the orchestrator. Open item O-18.

Decision taken meanwhile: the PKNCA-compatible default stays, with the flag; a consistent alternative is not offered (O-18).

Answer: _pending_.

## Q-011 (2026-10-08, oracle): how to drop the exports of the reference software

The private oracle (task T-019) reads `private/exports/td1/` and the coursework data in `private/coursework/`. Nothing there yet except the data. When you export, please:

1. Export the NCA "Final Parameters" table and the "Summary" table as text (CSV or tab-separated; the decimal separator and the delimiter do not matter), once for each of the two AUC methods (linear trapezoid with linear interpolation; linear-up/log-down), with the settings of the exercise otherwise left at the defaults.
2. Put the four files in `private/exports/td1/`. Name them so that the name says the table and the method, for example "final parameters linear.csv", "summary linear.csv", "final parameters lin up log down.csv", "summary lin up log down.csv". If the names are something else, add `private/exports/td1/manifest.json`: a list of `{"file": "<name>", "method": "linear" or "lin_up_log_down", "table": "final" or "summary"}`.
3. Export with the displayed precision you want compared: the test accepts a value when the engine's value, rounded to the number of decimals written in the export, is the written number. A table exported with more digits is a stricter test.

What the loader assumes (all `assumed` until the first export is read): a table with a header row; either one row per parameter (a name column and a value column) or one row per profile or statistic (parameter names as column headers, an optional units row under them, the row labelled mean used for a summary of one profile); export parameter names are mapped to the project's names by a small table in `crates/caladrius-testkit/src/private.rs` (`canonical_name`); a name it does not know is counted as "not mapped", never as a failure.

Decision taken meanwhile: with no export in the folder, the private tests print a "SKIPPED" line with this explanation and pass; `cargo xtask conformance` prints private counts on the console when exports exist and writes them (counts only) into `docs/conformance.md` only with `--private`, because publishing a numeric comparison with the reference software is your decision (AGENTS.md section 2).

Answer: _pending_.

## Note on Q-008 (2026-10-08, reader): what the screenshots already answer

No action needed. Reading the thirteen screenshots of `private/screenshots/` (allowed by `AGENTS.md` section 7) the reader can answer part of Q-008 in words, without numbers: (1) four calculation methods are offered (linear trapezoid with linear interpolation, log trapezoid, linear-up/log-down, mixed linear/log) and the one shown as selected is the linear trapezoid; (3) the λz rules page has a maximum number of points and an earliest start time, and an acceptance block (adjusted R², percent extrapolated AUC, span, number of samples, percent extrapolated AUC in a dosing interval), all blank, so no acceptance threshold is applied unless typed; the rule for values below the quantification limit is a separate project object that was not screenshotted, and the Tmax and C0 conventions were not shown (items 2, 4 and 5 stay open). A new point: the NCA setup also offers a weighting (user-defined, uniform, 1/Y, 1/(Y·Y)), uniform being selected; `specs/nca.md` has no weighted λz regression (open item UX-O-01 of `specs/ux.md`).

Decision taken meanwhile: `specs/nca.md` keeps the PKNCA profile as the application default for the AUC method (lin-up/log-down) until the orchestrator decides; the observed default of the reference (linear trapezoid) is a candidate for the reference profile of section 2.2 and for the default of the application (open item O-06). Orchestrator: please say whether to update section 2.2 accordingly.

Answer: _pending_.

## Q-011 (2026-10-08, reader): a sample file for the import heuristic

For `specs/ux.md` UX-IMP-01 and UX-O-02: the human reported a CSV with decimal commas that was imported with the comma taken as the separator. To tune the plausibility checks the reader needs the first ten rows of such a file, with the actual separator and decimal marks, but with every number replaced by a harmless one (nothing from `private/` may enter a versioned file). Alternatively describe in words what the file looked like (separators, header, how many columns).

Decision taken meanwhile: the heuristic of UX-IMP-01 stays as written, tagged `assumed`; an import that is ambiguous is never executed silently.

Answer: _pending_.

## Q-012 (2026-10-08, reader): a small private oracle for the open NCA conventions

Follow-up to the note on Q-008 (T-023). Observed results are an allowed source and cost no licence risk. To settle the open items O-06, O-20, O-21 and O-22 of `specs/nca.md`, please run one public profile through the NCA of the reference software: subject 1 of the theophylline data in `oracle/data/theoph.csv` (oral, dose in the `dose` column, no zero concentration), and export the result tables to `private/exports/`.

1. Once with each of the four calculation methods (keep the weighting uniform): AUC to Tlast, AUC to infinity, AUMC, and the λz window.
2. Once with each of the four weightings (user-defined excluded; keep the default method): λz, adjusted R², the first and last time used.
3. Once with the best-fit rules set to a maximum number of points of 4, then with the earliest start time set to 5 h (default method and weighting): the λz window.
4. Once with the curve-stripping check box ticked and once unticked: any parameter that changes.
5. If you also want the BLQ rule settled: a copy of the profile with the last two concentrations set to 0, run with the reference's default BLQ rule, and tell the orchestrator which rule the project object held.

Nothing from `private/` is copied into a versioned file; the orchestrator or the oracle agent compares the exports and records only agreement or difference.

Decision taken meanwhile: `specs/nca.md` keeps the reference profile apart from the PKNCA profile; the observed facts are tagged `observed`; the engine's default AUC method stays the PKNCA one until the orchestrator decides.

Answer: _pending_.
