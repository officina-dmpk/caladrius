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

## Q-002 (2026-10-08, orchestrator): comparing results with a commercial program

Closed 2026-11-01 (T-129). The comparison this question was about is removed from the repository. No conclusion is drawn here about what an academic licence permits. The answer recorded on 2026-10-08 is not relied on.

## Q-003 (2026-10-08, orchestrator): R packages

T-003 needs `PKNCA` installed in R (`install.packages("PKNCA")`). `AGENTS.md` allows installs through package managers, so the oracle agent will do it unless told otherwise.

Answer (2026-10-08, human): ok, install PKNCA.

## Q-004 (2026-10-08, oracle): license of the public datasets and of PKNCA output

T-003 adds `Theoph` and `Indometh` (R `datasets` package, distributed with R under GPL-2 or GPL-3) as CSV files in `oracle/data/`, and expected values computed with PKNCA (AGPL-3). The numbers are experimental measurements and computed results, but the CSV files are derived from files distributed under a copyleft license, while Caladrius is MIT OR Apache-2.0.

Decision taken meanwhile: keep the data and expected values in `oracle/`, list them in `ATTRIBUTION.md` with their origin and license, copy no code or documentation text from R or PKNCA, and keep `oracle/` out of any published crate (it is not under `crates/`). To confirm before the first release: either keep as is, or replace the CSV files by a pointer plus a script that regenerates them from R at test time.

Orchestrator note (2026-10-08): decision stands. Measured concentrations are facts, not code; R's GPL covers the package, PKNCA's AGPL covers its code, neither covers computed output. `oracle/` is not part of any published crate. To confirm by the human before the first public release.

Answer (human): pending.

## Q-005 (2026-10-08, reader): terms of a commercial product's public documentation

Closed 2026-11-01 (T-129). That documentation is not a source. The notes of what its terms say, and of what a search summary paraphrased, are removed from `specs/sources.md`. No conclusion is drawn here about what those terms permit.

## Q-006 (2026-10-08, reader): vendor and product names in `specs/sources.md`

Closed 2026-11-01 (T-129). The terms-of-use record that contained a vendor name is removed. The names stay in the README trademark notice, in the contract's ban, and in the lint that enforces the ban and the vendor-export guard.

## Q-007 (2026-10-08, reader): a published comparison table

Closed 2026-11-01 (T-129). The statement that the rules reproduce a published comparison table is removed (hand check H3). No number from that paper is in a versioned file. The licence of the paper is not ruled on, because the comparison is not used.

## Q-008 (2026-10-08, reader): defaults to read off a screen

Closed 2026-11-01 (T-129). Defaults are the ones specified from PKNCA and the engine. Notes of what was seen on a screen are removed. No further observation is requested.

## Q-009 (2026-10-08, reader): fitting defaults to read off a screen

Closed 2026-11-01 (T-129). Fit behaviour is specified from papers, from R `nls`, and from the engine. Comparing another program's setup is not a task.

## Q-010 (2026-10-08, reader): replaced values and the extrapolation to infinity

Closed 2026-11-01 (T-129) as to any other program. The engine keeps the PKNCA-compatible tail and raises `area_past_tlast` (`specs/nca.md` NCA-EXT-01b). Whether another program counts the stretch twice is not asked.

## Q-011 (2026-10-08, oracle): how to drop vendor exports

Closed 2026-11-01 (T-129). There is no private export oracle. Do not add vendor exports, project files or screenshots.

## Note on Q-008 (2026-10-08, reader): screenshots

Closed with Q-008 (T-129). The screenshot notes are removed.

## Q-011b (2026-10-08, reader): a sample file for the import heuristic

Closed 2026-11-01 (T-129). The import heuristic stays as written in `specs/ux.md`. No sample taken from a vendor file is requested.

## Q-012 (2026-10-08, reader): a private oracle for open NCA conventions

Closed 2026-11-01 (T-129). The campaign to settle conventions by running a profile through another program is withdrawn. The later note that an export had been compared is withdrawn with it: that comparison is not a result the project stands on.

## Q-013 (2026-10-08, orchestrator): publication

The human decided on 2026-10-08: publish the repository on GitHub, public, under a new organisation `officina-dmpk` (repository `caladrius`). The organisation is created by the human on github.com; the orchestrator then adds the remote and pushes `main`. Every later push is covered by this decision for this repository. A numeric comparison with a commercial program is not in this repository (T-129).

Answer (human): confirmed (chat, 2026-10-08).

## Q-014 (2026-10-09, orchestrator): default fit settings: the looser preset, or the exact minimum?

T-029 and T-030 measured, on all 185 reference fits (490 estimates), how far the engine's DEFAULT settings land from the exact minimum reached by R (`nlsLM`/`nls`, analytic derivatives, tight stop):

| setting | estimates beyond 1e-4 | worst gap |
|---|---|---|
| forward differences 1e-3, reference stopping rule | 40 | 6.9e-4 |
| forward differences 1e-5, reference stopping rule | 0 | 1.1e-5 |
| forward differences 1e-3, default stopping rule (relative decrease 1e-4) | 168 | 5.8e-3 |
| analytic derivatives, default stopping rule | 149 | 5.1e-3 |

The defaults (forward differences with increment 0.001, convergence 0.0001, 50 iterations) are the optional preset `reference_conventions`. Keeping them means a user who clicks "Fit" gets estimates accurate to about 5e-3 relative; the oracle tests, which run with analytic derivatives and the tight stop, are unaffected either way. Options:

1. Keep the defaults; state in D-03, `specs/fit.md` and the README that default fits are accurate to about 5e-3, and show the "derivatives" and "convergence" options in the UI with a sentence.
2. Default to analytic derivatives (available for every v1 model in the (v, k) parameterisation since T-030) and a tighter stop (relative decrease 1e-8), and document the difference as D-04. Estimates then match R within 1e-4 by default.

Decision taken meanwhile: option 1 for v0.1.0 (no behaviour change before the tag); the texts say so. Your answer decides v0.2.

Answer (2026-10-09, human): option 2. The default fit uses analytic derivatives and the tight stopping rule; the other iteration settings stay available as the named preset `reference_conventions`. Card T-040.

## Q-015 (2026-10-29, orchestrator): remove the user-level `/onizuka` command?

Since T-115, Onizuka has one place: `.claude/agents/onizuka.md` in this repository, the master copy. The French command `C:\Users\abdou\.claude\commands\onizuka.md` (235 lines, an earlier version) is still offered in every Claude Code session of the PC, other projects included. It lies outside every repository, so section 11 needs written agreement before it is deleted. Options: (1) delete it; (2) keep it as the human's own shortcut, knowing that it is no longer updated.

Decision taken meanwhile: kept, untouched. `~/.claude/commands/plume.md` stays where it is either way (one place already; it holds the owner's biography and personal texts, which stay out of public repositories).

Answer (2026-10-29, human, in chat): "puis tu supprimes Q-015", read by the orchestrator as option 1: delete the user-level command. Not done by the orchestrator: the session's permission check refused the deletion of a file outside every repository, and it was not worked around. Abdou deletes it by hand (`rm C:/Users/abdou/.claude/commands/onizuka.md`, or `Remove-Item` in PowerShell); until then it stays, no longer updated.

## Q-016 (2026-10-29, orchestrator): merge and push the branches of T-113 to T-118

`contract/iterations` (this repository), `onizuka/one-place` (apothicaire) and `method/iterations` (officina umbrella) were committed and not pushed, as asked. Q-013 covers pushes of this repository only.

Answer (2026-10-29, human, in chat): "on fusionne et push d'abord". The three branches are merged into their `main` and pushed. A push of the umbrella's `main` deploys the site only when `site/**` changes; `METHOD.md` does not.

## Q-017 (2026-10-29, orchestrator): hosted-model runs and rule 8 ("No paid service. No billed API")

The audit of T-119 to T-122 found that `../agent/bench/FRONTIER_ARM.md` and `../agent/bench/frozen/LEDGER.md` (2026-10-22) record three runs of the agent on hosted models, in a cloud session: 12.17, 20.96 and 10.30 USD, 43.43 USD in all. `AGENTS.md` section 10 rule 8 forbids any billed API, and no entry of this file authorises them. Options: (1) record that Abdou authorised these runs, as an exception for the frontier arm of the frozen set, with a budget; (2) amend rule 8 so that a billed API is allowed for a measurement Abdou approves in writing, never in the product; (3) stop the frontier arm. Meanwhile the READMEs say "nothing leaves the machine" only of the assistant at run time (T-119, T-122).

Answer (2026-10-29, human, in chat): the runs used free credits offered by Anthropic, so no money was spent; Abdou knew the rule was set aside. Read by the orchestrator as option 1: an exception recorded here for the frontier arm, on free credits only. Rule 8 is unchanged: any later billed run needs the same written agreement in this file first.

## Q-018 (2026-10-29, orchestrator): the family name on the GitHub profile

T-104 (Abdou's request of 2026-10-22) removed the family name from the four repositories, and `.claude/agents/onizuka.md` says it never appears, anywhere. The public profile `abdou-rgd/abdou-rgd` starts with the full name (`### Abdallah ...`), as does its copy `../notes/github-profile-README.md`. Options: (1) keep it on the profile only, which is Abdou's own page, and say so in the rule; (2) replace it with « Abdallah » or « Abdou ». Untouched until the answer.

Answer (2026-10-29, human, in chat): "il faut retirer mon nom de famille", option 2. The profile now says « Abdallah » (abdou-rgd/abdou-rgd ff2ecf1, pushed), and its copy in `../notes/` too. Still there, outside the text: the author e-mail of past commits in every public repository, which only a history rewrite would remove; future commits can use the GitHub no-reply address if Abdou changes the git setting.

## Q-019 (2026-11-06, orchestrator): push the prototype branches of iteration 2

Iteration 2 (T-130) builds three prototypes of the NCA screen on branches (`iter2/base`, then one branch per stack). Q-013 covers pushes of this repository after a closed task; these branches are pushed before any card closes, so that CI can render them on Linux. Nothing of them merges into `main` before 2026-11-18.

Answer (2026-11-06, human, in chat): "oui pour pousser les branches". The branches of T-133 to T-137 may be pushed. Merging into `main` still waits for the decision of T-138.

## Q-020 (2026-11-07, orchestrator): amend `AGENTS.md` for iteration 3 (T-139): the interface target and the population scope

Iteration 3 brings three compartments, population fitting and population simulation into the engine, and records the stack verdict of T-130 (`board/messages/2026-11-07-fable-stack-investigation.md`). Without points 3 and 4 the contract forbids bricks 1, 3 and 4: line 21 puts NLME out of scope "not even to prepare", and line 11 says the contract wins over any task. Each point is today's text, then the proposed text; answer per point (yes, no, or your wording).

1. **Line 5, what Caladrius is and its interface.**
   Today: "Caladrius is an open-source pharmacokinetic analysis application written in **Rust only**: non-compartmental analysis (NCA), individual compartmental models, least-squares fitting, plots. Native UI in egui (eframe on wgpu); the same code compiles to WebAssembly for the demo. **No Tauri, Electron or webview.** License: MIT OR Apache-2.0."
   Proposed: "Caladrius is an open-source pharmacokinetic analysis application written in **Rust only** (a web front adds HTML, CSS and the JavaScript glue that wasm-bindgen generates, no JavaScript framework): non-compartmental analysis (NCA), compartmental models, individual and population fitting, simulation, plots. The reference interface is egui (eframe on wgpu), native and in WebAssembly, until its successor does everything it does. The successor is Leptos 0.8 in Tauri 2, under the three conditions of `board/messages/2026-11-07-fable-stack-investigation.md`: the engine never runs on the display thread; long commands report progress and can be cancelled (`docs/long-running-commands.md`); Leptos's maintenance is re-read at each of its releases, with Dioxus as the named fallback and nothing Leptos-specific outside the view layer. The browser without Tauri stays a target: the engine in WebAssembly in a Web Worker. **No Electron.** License: MIT OR Apache-2.0."
   Consequence: the TypeScript client of T-134 (branch `iter2/base`) does not reach `main` under this wording; the Leptos front gets a Rust client.

2. **Line 71, the layers.**
   Today: "A crate depends only on lower layers. Nothing below `caladrius-ui` knows egui, eframe or winit. `cargo xtask layers` checks this; every new crate is registered there."
   Proposed: "A crate depends only on lower layers. Nothing below the interface layer (L3: `caladrius-ui` today, the Leptos front and the Tauri shell later) knows egui, eframe, winit, Leptos, Tauri, `wasm-bindgen`, `web-sys`, `js-sys` or a webview; the fronts, the shell and the bridge live in `apps/`. `cargo xtask layers` checks this; every new crate is registered there."
   Consequence: `cargo xtask layers` learns the new names, on an `iter3/*` branch (code, so not on `main` before 2026-11-18).

3. **Section 1, lines 19 and 21, the scope.**
   Today, line 19: "Only afterwards, in this order: two compartments, user-written models (ODEs), several subjects. Start an item only when `docs/conformance.md` is green for everything before it."
   Proposed: "Then, in this order: two compartments (done), three compartments, several subjects with their own doses, population modelling (NLME: FOCE with interaction and its diagnostics), population simulation, user-written models (ODEs). Start an item only when `docs/conformance.md` is green for everything before it."
   Today, line 21: "Out of scope: population modelling (NLME), bioequivalence, IVIVC. Do not start them, not even "to prepare"."
   Proposed: "Out of scope: bioequivalence, IVIVC, and any population estimator other than the one named above until it is green. Do not start them, not even "to prepare"."
   If brick 2 keeps Michaelis-Menten elimination, it joins line 19 before population modelling as "one built-in ODE model (Michaelis-Menten elimination)".

4. **Rule 10 (line 43), a seeded stream for simulation.**
   Today: "...so an answer is a function of the parameters and of the project alone: no clock, no random number, no path and no fact about the machine in it..."
   Proposed: "...so an answer is a function of the parameters and of the project alone: no clock, no random number except a stream drawn from a seed that is one of the parameters, by a generator written in the engine and specified in `specs/`, no path and no fact about the machine in it..."

5. **Sections 4 and 5, the map and the tolerances.**
   Line 50, today "closed-form solutions: 1 then 2 compartments, bolus, ...", proposed "closed-form solutions: 1, 2 and 3 compartments, bolus, ...".
   Line 53, after "minimization trace", add "; population fitting (FOCE with interaction), its diagnostics, population simulation".
   Line 77, after the PKNCA sentence, add "population fits by nlmixr2, with saemix and `nlme` as second looks, from versioned R scripts; never a commercial program."
   After line 83, add one line: "- Population fitting: the tolerances are set in `caladrius-testkit` by the oracle card of iteration 3 from what it measures, asked in this file before any engine card uses them, then written here."

6. **`README.md` line 3, the interface sentence.**
   Today: "...with a native desktop UI (egui) and the same interface in a browser page (WebAssembly)."
   Proposed: "...with a native desktop UI (egui) and the same interface in a browser page (WebAssembly); egui stays the interface until its planned successor, Leptos in Tauri 2, does everything it does."
   The scope words of the same sentence ("individual compartmental models, weighted least-squares fitting") change only when population fitting reaches `main`, after 2026-11-18.

Decision taken meanwhile: nothing applied. On a yes, the orchestrator applies the agreed points in one commit on `main` (contract and README text, no code: the demo of 2026-11-17 is unaffected), with `cargo xtask lint` and `review` green and `AGENTS.md` under 200 lines (179 today; the points add about two). The roles table (section 8) is unchanged: the model of each card is chosen on the card of T-139.

Answer (2026-11-07, human, in chat): "oui lance, je vais m'en aller donc je te laisse gérer tout ça. pour l'interface, on refera des tests quand on aura intégré la popPK qui nous permettra de vraiment trancher sur quelle stack on part". Read by the orchestrator as a yes to the iteration and to the engine points 3, 4 and 5, applied on `main` the same day. Points 1, 2 and 6 (the interface) are held: Abdou decides the stack on the tests that follow population PK (T-130, wave 4), so line 5, line 71 and the README keep today's text until then. Line 5 still says "individual compartmental models" and "No Tauri, Electron or webview"; iteration 3 touches no interface, so nothing it does conflicts with it.

## Q-021 (2026-11-07, orchestrator): install Rtools 4.5 for the nlmixr2 oracle

nlmixr2 is the open oracle of the population fit (T-139, brick 3). It compiles each model to C through rxode2, which on Windows needs Rtools 4.5 (free, distributed by CRAN, with its own installer). Section 11 forbids installing software outside the package managers without written agreement here; the R packages themselves install through `install.packages`, which is allowed. On this PC (2026-11-07): R 4.5.2 with `nlme` 3.1.168; no nlmixr2, rxode2, saemix or Rtools. Options:
1. Install Rtools 4.5 on this PC; the oracle scripts run here like the PKNCA ones.
2. Run the nlmixr2 scripts in a cloud session (Linux has a compiler) and commit only their versioned outputs.
3. No nlmixr2: saemix and `nlme` only. The OFV at fixed parameters would then have no external oracle, only our naive implementation and the cases where the Laplace approximation is exact.

Decision taken meanwhile: option 1 is recommended; until the answer, the oracle card installs saemix with `install.packages` and writes the nlmixr2 scripts without running them.

Answer (2026-11-07): not answered in words. "je te laisse gérer tout ça" is not read as a yes to download and run an installer outside the package managers; the decision taken meanwhile stands. Rtools is needed only from wave 6 of T-139.

## Q-022 (2026-11-07, orchestrator): push the branches of iteration 3

Iteration 3 (T-139) keeps its code on branches `iter3/*` until 2026-11-18. CI checks them on Linux, Windows, macOS and wasm32 only once they are pushed, as Q-019 allowed for `iter2/*`. Q-013 covers pushes after a closed task only.

Decision taken meanwhile: the `iter3/*` branches stay local; `main` receives only `docs/` and `board/`, pushed under Q-013.

Answer (2026-11-07): not answered in words; the decision taken meanwhile stands.

Answer (2026-11-08, Abdou, in chat): « Oui, pousse-les ». The 16 `iter3/*` branches were pushed to `origin` on 2026-11-08 as they stood, finished work and work in progress alike (`iter3/pop` carries T-154, which has not been reviewed). Pushing a branch is not a merge: `main` still receives only `docs/` and `board/` before 2026-11-18. The untracked T-174 scripts in the `pop-triage` worktree were not committed, so they were not pushed. Later that day, during the cleanup, they were committed as they stood (963dbd1 on `iter3/pop-triage`) and pushed.

## Q-023 (2026-11-07, orchestrator): the tolerances of the maximum-likelihood fit in `AGENTS.md` section 5

T-148 (the oracle of brick 2) needed tolerances that section 5 does not have: −2LL crosses zero, so a relative rule on it means nothing. It defined them once in `caladrius-testkit` (`AbsoluteTolerance`, `crates/caladrius-testkit/src/ml.rs`), and only the new tests use them. They agree with what `specs/fit.md` OF-12 proposed from the reader's prototype (−2LL to 1.3e-9 against `optim`).

Proposed line, after "Fitting: ..." in section 5 (181 lines today, one line added):

"- Maximum-likelihood fitting (`objective = ml`): −2LL, AIC and BIC within 1e-7 absolute (a log-likelihood can be negative, so a relative rule means nothing), IWRES within 1e-5 absolute, a value that is zero by structure within 1e-8 absolute; estimates and standard errors as for fitting."

Decision taken meanwhile: the constants stand in the testkit for the new tests and T-149 uses them as they are, reporting the worst error it reaches for each. Nothing is written in `AGENTS.md` without a yes here.

Added by the orchestrator after the T-148 review (2026-11-07): the reviewer measured a coupling. A −2LL within 1e-7 of the optimum keeps a parameter within 1e-4 relative only when its CV is below about 32 %, and 46 of the 436 CVs of the oracle are above it. IWRES at 1e-5 needs σ to about 4e-6 relative. Nothing is changed before T-149 measures what it reaches. If a correct engine misses only on weakly determined parameters, the question becomes whether to accept a fit by its likelihood (−2LL within 1e-7, and the parameters inside the matching profile region of the 256-bit Hessian) rather than by a fixed relative gap.

Added by the orchestrator after T-149's fix round (2026-11-07): with the Newton polish at the end of the iteration, the engine reaches these worst errors at eae738c, against the tolerances proposed above:
- −2LL: 6.04e-14 absolute;
- estimates, SE and correlations: 4.85e-9 relative;
- IWRES: 9.09e-8 absolute;
- a structural zero: 2.26e-16 absolute.

Every one is at least about 200 times inside. Without the polish, 23 of 477 tests miss, mostly combined fits with a tiny `error_b`. So the coupling the review measured does not bite a converged fit. The proposed line stands as written.

## Q-024 (2026-11-07, orchestrator): a condition-scaled tolerance for the ill-conditioned scalars of the three-compartment models

T-142's oracle marks 47 tests `#[ignore]`. They are scalars (micro constants, weights, conversions) whose condition number κ, written in each case, exceeds 1e3. On them no double-precision formula can promise 1e-12 relative (MOD-3C-08).

T-144 ran them against the engine at fd0d5d4. 46 pass at the usual `MODEL_VALUES`. One fails, `conversions_ill::conv_tie_6_clearance::scalars`:
- two rows, `b` and `w_beta`, are off by 1.05e-10 relative;
- κ is about 8.0e6, so κ·2⁻⁵² = 1.78e-9, and the error is 0.059 of it, about 17 times inside.

Section 5 has no rule for this case. Two options:
- **(a)** Keep the 47 tests ignored and informational, as today. Nothing changes in `AGENTS.md`.
- **(b)** Add one line to section 5 and turn the 47 into real tests: "Scalars whose condition number κ, written by the oracle, exceeds 1e3: relative error within κ·2⁻⁵²." No other tolerance moves.

The orchestrator recommends (b). It tests the engine where the conditioning limit lies, and the measure shows a margin of about 17. Nothing is applied without a yes here.

## Q-025 (2026-11-07, orchestrator): the open choices of the population fit (`specs/population.md`, OP-01 to OP-14)

T-162 wrote `specs/population.md` on branch `iter3/pop` (95d620d), and a Fable review read it. Section 11 of the spec lists 14 open items.
- **For OP-01 to OP-10 and OP-14 you choose.**
- **OP-11 to OP-13 are numbers:** the oracle (T-152) measures them and proposes values, which come back here before they become rules.
- **OP-02 is settled by measurement**, once the reference tool runs (Q-021).

Where the review proposed something else, both proposals are given. Every line is a proposal, and nothing is applied without your answer. Two of them are needed before T-153 starts: OP-01 and OP-14.

| item | the spec proposes | the review proposes | the orchestrator recommends |
|---|---|---|---|
| **OP-01** Ω in v1 (before T-153) | diagonal by default, plus one full block as an option | the same | the spec's proposal |
| **OP-03** the OFV constant | `full` (2π kept) by default, the convention named in every answer | the same | the spec's proposal |
| **OP-04** the default covariance | `sandwich` (R⁻¹SR⁻¹) | `sandwich` printed, and all three returned | the review's: R and S are computed anyway, and with few subjects the spread is the useful number |
| **OP-05** the n of BIC | the number of observations, as in the individual fit | the same | the spec's proposal, with n written in the answer |
| **OP-06** the form of CWRES | diagonal (independent of the order of the observations) | decorrelated by the Cholesky factor, in time order (DAT-ORD-01), with the diagonal form as a second column | the review's: its sum of squares equals the distance term of the OFV, an exact check, and the order is fixed by DAT-ORD-01 |
| **OP-07** BLQ | exclusion (M1) in v1; the censored likelihood (M3) later | the same | the spec's proposal; whether M3 is the next card after T-155 is yours |
| **OP-08** η on `tlag` or `dur` | refused in v1 | the same | the spec's proposal |
| **OP-09** several doses per subject | refused in v1 | the same | the spec's proposal |
| **OP-10** the shrinkage divisor | m − 1 | the same | the spec's proposal |
| **OP-14** plain FOCE, without interaction (before T-153) | `later` | in v1: a one-line switch, the default of the reference's `posthoc` method, and with the identity of POP-OBJ-04 an exact check that separates a wrong formula from a wrong interaction | the review's |

Also open, with no choice needed from you now:
- **OP-02:** which objective formula the reference computes. The OFV at fixed parameters against nlmixr2 decides it, so it waits on Q-021.
- **OP-11:** the algorithm constants.
- **OP-12:** the method tolerances.
- **OP-13:** the design of the sealed simulate-then-fit tests.

T-152 proposes each value in its report, and each comes back here as its own question.

Decision taken meanwhile (2026-11-08): Abdou wrote in chat "oublie les deux agents en mêmes temps fais ce que tu veux tu as tout les droits, à la fin éteint le pc stp". This is read as a delegation of the open choices for this iteration. So the orchestrator's column above applies:
- the spec's proposal for OP-01, OP-03, OP-05, OP-07, OP-08, OP-09 and OP-10;
- the review's for OP-04, OP-06 and OP-14.

Every one is reversible by Abdou. T-166 writes them into `specs/population.md` before T-153 codes from it. Two points that the table leaves open are also decided meanwhile:
- **OP-13, design B** (`limits`, sparse data and a large ω): recorded only, until T-153 has measured it. The orchestrator then proposes a bound here.
- **The σ point of plain FOCE** (T-152, question 1): T-166 settles it from an open source. Until then, T-153 builds FOCEI (with interaction) and FO only. The switch to FOCE without interaction follows the spec sentence.

## Q-026 (2026-11-08, orchestrator): the tolerances of the population fit

T-152, the population oracle, proposed five absolute constants. They are defined in `crates/caladrius-testkit/src/pop.rs` on `iter3/pop` (c0a4a1d), and only the new tests use them. No existing constant is loosened, and each value comes with its measurement:

| constant | value | for | the measurement behind it |
|---|---|---|---|
| `POP_OFV` | 1e-7 absolute | the OFV total and per subject, AIC, BIC | as `FIT_M2LL` (Q-023); the OFV crosses zero; the independent naive code is within 2.9e-12 of the 256-bit totals |
| `POP_ETA` | 1e-10 absolute | the conditional modes η̂ | the linearised OFV is not stationary in η, with a measured slope of up to 37.4 per subject on Theoph (12 subjects); holding `POP_OFV` needs 2.2e-10 or less |
| `POP_DIAGNOSTIC` | 1e-5 absolute | IPRED, IWRES, the three CWRES | as `FIT_IWRES`; the amplification from η stays below 1e3 in these cases |
| `POP_DERIVED` | 1e-7 absolute | η- and ε-shrinkage | the smallest ω in the cases is 0.04 |
| `NAIVE_POP_OFV` | 1e-9 absolute | the testkit's naive code against the stored values | measured at 2.9e-12 at worst |

The method tolerance of the population estimates (POP-EST-06, OP-12) has no number yet. With no engine to measure it, a number would come from nothing. The proposal is to measure bias and coverage on the sealed design `accurate` (200 replicates) after T-153, and to come back here with that measurement.

Decision taken meanwhile: the constants stand in the testkit for the new tests, and T-153 uses them as they are and reports the worst error it reaches for each. Nothing is written in `AGENTS.md` without a yes here.

Measured since (T-152 fix round A, 2026-11-08):
- **`POP_DIAGNOSTIC`:** the amplification max |∂ŷ/∂η|/σ is 3.24e2 over the stored cases, so an η within 1e-10 moves a diagnostic by at most 3.2e-8.
- **`POP_ETA`:** in the fully pessimistic sum for q = 3 on `theoph_proportional`, 1e-10 on η allows 1.3e-7 on the OFV, which is 1.3 times `POP_OFV`. The power-of-ten margin holds for q = 1 and 2 only. The OFV is compared directly in every case, so a wrong η that moves the OFV is caught there.
  - Option (a), the proposal: keep 1e-10.
  - Option (b): 3e-11, which restores the margin. It is stricter for the inner solver.
  - Decision meanwhile: (a), with T-153 reporting its worst η error.

Added by T-168, the exact fit-level oracle (`iter3/pop`, 07b81f1 and bd19a46). No new constant is proposed; the rules below use `POP_OFV`.
- **Parameters at the maximum:** |d_j| ≤ √(2·POP_OFV·(R⁻¹)_jj), with the diagonal of R⁻¹ stored per case (`tolerance_basis`).
  - This is the Cauchy–Schwarz bound for an OFV excess of `POP_OFV` along a quadratic.
  - The bounds run from 1.3e-5 to 8.0e-4 depending on the label.
  - Against the worst disagreement of the three references (256-bit Rmpfr, nlme ML, direct maximisation), the margin is at least 1000× on the free parameters and 22× to 70× on the Ω of the rank-one block.
- **The objective at the maximum:** `POP_OFV`.
  - Measured 8.5e-14 at worst on the interior cases.
  - On the boundary cases it is 4.8e-8 and 2.8e-8, a margin of 2, which comes from nlme's convergence residue. The 256-bit point is stationary to 1e-74.
- **A variance held on 0:** at most `POP_OFV`/gradient, which gives 5.4e-9 on `q1_boundary` and 1.0e-9 on `q2_diag_boundary`.
  - The engine works on ln ω, so it must also report `omega_collapse` or `at_bound`.
  - The margin to the references is 5 and 2.
- **Margins below 10× are engine requirements, not tolerances to widen:** ε_out ≤ 5e-8, and finite-difference noise below about 1e-8 at ln ω about −9.5 and −10.3. Both are written in T-154.
- The standard errors (r, s, sandwich) are printed and never asserted.

Added by T-158, the population-simulation oracle (`iter3/sim`, 5b6a184 to e6e5b4a). These constants live in the testkit and only the new tests use them:

| constant | value | for | the measurement behind it |
|---|---|---|---|
| `TOL_Z` | 1e-14 relative | z in the tails of AS 241 (the central region is bit-exact) | 7.0e-16 measured, with `ln` moved by ±1 ulp; margin 14 |
| `TOL_EXACT` | 1e-12 relative to max(\|e\|, scale) | η, P, PRED, IPRED, σ, y_raw, y, dv of the exact part | 9.48e-16 measured; margin about 1055 |
| `TOL_STAT` | 1e-9 | the statistics of the distribution checks recomputed from stored draws | 9.86e-14 measured |

Bit-exact, with no tolerance: the KATs, the counters, the words, the uniforms, L, the central z, the type 7 quantiles, the counts and `cens`. The BLQ and zero ties cannot flip on a correct engine: the nearest y is 2e-3 of the scale from LLOQ and 1e-3 from 0.

Decision taken meanwhile: the T-168 and T-158 rules stand for the new tests. Nothing is written in `AGENTS.md` without a yes here.

## Q-027 (2026-11-08, orchestrator): the AS 241 constants in a published crate

T-159 writes the inverse of the normal distribution (AS 241, Wichura 1988, Applied Statistics 37(3):477-484) in `crates/caladrius-fit/src/sim/normal.rs`, on `iter3/sim`. Its 48 coefficients come from the table in `specs/simulation.md`. That table was read from R's `src/nmath/qnorm.c`, which is under GPL-2 or GPL-3. The code itself is written from the spec, not taken from R. The T-159 review (Fable) checked that the 48 coefficients are the same doubles as the spec table.

Q-004 does not cover this case. It rests on `oracle/` being outside every published crate, but `caladrius-fit` is published under MIT OR Apache-2.0.

Decision taken meanwhile (2026-11-08), reversible by Abdou: keep the constants. They are the published coefficients of a published algorithm, numbers rather than code. `ATTRIBUTION.md` cites Wichura (1988) and points here.

To confirm before the first release. Option (a): keep as is. Option (b): check the coefficients against the printed table of the 1988 paper and cite only the paper. Option (c): replace AS 241 with another published inverse.

Answer (human): pending.

## Q-028 (2026-11-08, orchestrator): the cleanup before the change of how the project is run

`AGENTS.md` section 11 asks for a written agreement before deleting files outside the repository and before any `git push`. The cleanup of 2026-11-08 does both:
- it removes the worktrees under `C:\Users\abdou\caladrius-iter2` and `C:\Users\abdou\caladrius-iter3`, the agent build directories under `target/`, and an old scratchpad of this session;
- it deletes, locally and on `origin`, the branches already contained in a kept branch;
- it pushes `iter2/egui`, `iter2/react`, `iter2/leptos` and the two new commits of `iter2/base`.

Answer (2026-11-08, Abdou, in chat): « ok fait tout le process de nettoyage, maintenance, rangement aussi bien sur le repo qu'en local parce que je vais faire une grosse update sur comment est-ce qu'on dirige ce projet ». Nothing was deleted that a kept branch or `origin` does not still hold; the deleted branch tips are in `LOG.md`.

## Q-029 (2026-11-14, orchestrator): what the Fable review of T-154 leaves open (population estimation)

The review (`board/messages/2026-11-14-review-T-154.md`) found no wrong estimate, but asks for changes. Before the demo of 2026-11-16 no engine change is made in `crates/caladrius-fit/src/pop/`: each one costs a 12-minute verify and a Fable re-check, and the estimation is not reachable from any command yet (T-156 is not written). Blocker 1, the missing tests, is card T-188. The rest waits for Abdou:

1. **Blocker 2, POP-COV-04: no SE for the correlations of Ω.** Decision meanwhile: cut from T-154 and done in the engine fix round after the demo, by the delta method of POP-COV-04.
2. **The `s` kind under `omega_collapse`** (`covariance.rs:226-275`): the collapsed ln ω stays in the inverted set, and `s` is 9.9%, 15% and 71% from the oracle's conditional SEs on the three exact boundary cases. POP-COV-03 has no rule for it.
   - Option (a): leave the collapsed parameter out of the inverted set and print it as NC, which matches the conditional SEs the oracle stores.
   - Option (b): keep it and mark `s` NC whenever a collapse is flagged.
   - Decision meanwhile: none asserted; `reader` writes the rule after the demo. Recommendation: (a).
3. **The two-stage start with a combined error** (POP-EST-05): on `accurate` it leaves out 30 to 38 of 40 subjects (replicates 1, 2, 46, 97), because most individual fits end on a bound.
   - Option (a): keep the subjects whose individual fit ends `at_bound` in the means.
   - Option (b): use a proportional error for the individual stage.
   - Decision meanwhile: unchanged; a `reader` round after the demo. Recommendation: (a).
4. **Eleven "decisions taken meanwhile" live only in code comments**, and three contradict a spec sentence: `inner.rs:42` against POP-INN-02 (the polish condition), `outer.rs:63` against POP-COV-02 requirement 3, `mod.rs:758` against POP-EST-02 (a fixed ω = 0 is refused, the spec says it means no random effect). Decision meanwhile: the spec wins until a `reader` round amends it; the engine fix round aligns the code case by case, after the demo.

## Q-030 (2026-11-14, orchestrator): the sealed bias check of `accurate` fails for an exact minimiser of POP-OBJ-02

T-174 (verdict (a), reviewed): on the 200 sealed replicates of `accurate`, an independent R fit of the FOCEI objective of `specs/population.md` (POP-OBJ-02) agrees with the engine replicate by replicate and gives the same mean cl, 5.07955 against the truth 5 (limit 0.05382, z = 4.4). saemix, the exact likelihood by stochastic approximation, gives cl 5.0071 on the same data. The engine is not the cause; the check `sealed_recovery::accurate` fails because of the objective form. POP-ORA-04 forbids retuning the constants or the sealed data.
- Option (a): fit the 200 replicates in R with the other FOCEI forms of OP-02 (the three POP-OBJ-04 Laplace forms) and against a fresh draw under a new seed, then decide which form the engine follows. The bias check stays red meanwhile. About one oracle card.
- Option (b): record the shift as a documented method difference (`specs/differences.md`, D-xx: FOCEI against the exact likelihood on design A) and change the check of cl into a check against the R FOCEI reference, keeping saemix as the second look. Needs your yes, because it changes what the sealed test asserts.
- Decision meanwhile: nothing changes; the check stays red with its numbers. Recommendation: (a) first, then (b) for the forms that keep a shift.

Added to Q-026 by T-188 (2026-11-14, `iter3/pop` 9f93d0f): one constant for the standard errors, used only by `oracle_pop_review.rs`.

| constant | value | for | the measurement behind it |
|---|---|---|---|
| `POP_SE` | 1e-4 relative | the SEs of the three covariance kinds on the interior exact cases | worst engine error against the exact SEs, 3 cases × 2 starts: 1.34e-5 (sandwich), 1.98e-5 (r), 2.81e-5 (s); margin 3.6 |

It does not cover a collapsed ω, since POP-COV-03 has no rule for it (Q-029 point 2). Decision taken meanwhile: the constant stands for the new tests. Nothing is written in `AGENTS.md` without a yes here.
