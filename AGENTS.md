# AGENTS.md: contract for agents

Project name: **Caladrius** (crates are prefixed `caladrius-`; the bare `caladrius` name is free on crates.io and PyPI and is reserved for the desktop application). The name, the code, the docs and the UI never contain "Phoenix", "WinNonlin" or "Certara", except in a neutral statement of compatibility and the trademark notice in the README.

Caladrius is an open-source pharmacokinetic analysis application written in **Rust only**: non-compartmental analysis (NCA), individual compartmental models, least-squares fitting, plots. Native UI in egui (eframe on wgpu); the same code compiles to WebAssembly for the demo. **No Tauri, Electron or webview.** License: MIT OR Apache-2.0.

Caladrius is also the verified calculation layer of **Apothicaire**, a local DMPK agent developed in the parent folder: every Caladrius command must be callable by a machine (CLI and MCP server) with the same semantics as from the UI.

**Everything in the repository is written in English**: code, identifiers, comments, docs, commit messages, the board, UI text, and `board/QUESTIONS.md`. The human may write to you in French; answer in English.

Read this whole file before doing anything. If a task conflicts with this file, this file wins.

## 1. Scope

In scope (v1): data worksheet (CSV import), NCA, XY plots, one-compartment models (bolus, infusion, zero- and first-order absorption, tlag), weighted fitting, export of result tables, a CLI and an MCP server exposing the command registry.

Only afterwards, in this order: two compartments, user-written models (ODEs), several subjects.

Out of scope: population modelling (NLME), bioequivalence, IVIVC. Do not start them, not even "to prepare".

## 2. What the human does (closed list)

The human (Abdou) only:

1. Installs the reference software on the machine, outside the project folder.
2. Drops once into `private/`: exported result tables (CSV), his coursework data, and about fifteen screenshots of the reference software.
3. Answers the questions in `board/QUESTIONS.md`.
4. Decides every publication: pushing to GitHub, making the repository public, publishing any numeric comparison with the reference software.

Agents do everything else. Ask the human only when none of the rules below settles the matter; then write the question in `board/QUESTIONS.md` with the decision you took meanwhile, move to another task, and do not block.

## 3. Golden rules

1. **Clean room.** We reproduce behaviour, never code. Allowed sources: textbooks (Gabrielsson & Weiner, Gibaldi & Perrier), papers, the `PKNCA` documentation, the public user documentation of the reference software (section 6), observed results. Forbidden: opening the reference software's install folder, decompiling, copying code, icons, help text, wording of dialogs, or screenshots. Every third-party asset has its license and a row in `ATTRIBUTION.md` in the same commit.
2. **Tests are the gate.** No change without a test. A number outside tolerance fails the test. Conformance floors only go up.
3. **Engine first.** Everything is drivable without a window, from tests and the CLI. The UI is one client.
4. **Everything is a command.** Each user-visible action has a stable id (`nca.run`, `fit.run`, `plot.xy`, `data.import`…) and serializable parameters (serde). Menus, shortcuts, CLI, the MCP server and agents call the same command.
5. **Nothing baked into the core.** Units, route of administration, number of compartments, weighting, number of subjects, AUC rule: these are data, not assumptions, even if v1 exposes only part of them.
6. **Never crash.** Outside tests: no `unwrap()`, `expect()`, `panic!`, `todo!`, `unreachable!`, no `[i]` indexing on input-derived data, no `unsafe`. Every crate starts with `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]`. NaN, zero or negative concentrations, values below the limit of quantification, duplicated or unsorted times, non-estimable λz, singular matrix, non-convergence: each gives a readable error and a regression test.
7. **Thin, data-driven UI.** UI state is serializable. Colours, radii and spacing come from a theme token file (light and dark), never hard-coded.
8. **Look at what you draw.** Every UI change is rendered offscreen to PNG (`cargo run -p caladrius-ui --example snapshot`) and looked at before delivery.
9. **Never break wasm.** Layers L0 to L3 compile for `wasm32-unknown-unknown`. Platform-only code (files, threads, processes) is gated behind `cfg` and lives in `apps/`. Until step 6 of the marching order, `cargo xtask wasm` is advisory; from step 6 on, it is a gate.

## 4. Repository map and layers

```text
crates/
  caladrius-nca      L0  AUC (linear, log-linear, mixed), λz, t½, extrapolation, AUMC, MRT, CL/F, Vz/F, Vss
  caladrius-models   L0  closed-form solutions: 1 then 2 compartments, bolus, infusion, zero- and first-order absorption, tlag
  caladrius-fit      L0  weighted least squares (1, 1/y, 1/y², 1/ŷ, 1/ŷ²); outputs: final parameters (estimate, standard error,
                         CV%, univariate and planar confidence intervals), secondary parameters, diagnostics (corrected and
                         weighted sums of squares, residual SS, S, DF, correlation observed/predicted, AIC, SBC), correlation and
                         variance-covariance matrices, eigenvalues, condition numbers, partial derivatives, predicted data,
                         minimization trace
  caladrius-project  L1  project model: worksheets, workflow objects, results (pure data, serde)
  caladrius-engine   L2  command registry, history, export
  caladrius-ui       L3  egui interface
  caladrius-testkit      oracles, tolerances, table comparison
apps/
  caladrius          desktop application
  caladrius-cli      the same without a window
  caladrius-mcp      MCP server exposing the command registry to agents (Apothicaire)
xtask/               layers | wasm | conformance | ci
specs/               behaviour and UX specifications, written by us
oracle/              public data and expected results (versioned)
private/             reference exports, coursework data, screenshots (git-ignored)
board/               shared board for agents (section 9)
```

A crate depends only on lower layers. Nothing below `caladrius-ui` knows egui, eframe or winit. `cargo xtask layers` checks this; every new crate is registered there.

## 5. Oracle and tolerances

Three sources, in priority order for versioned tests:

1. **Public, in `oracle/`**: free datasets (theophylline, indomethacin) with `PKNCA` results produced by a versioned R script; worked examples from textbooks, with the page reference; published validation tables of other open or free NCA tools when their license allows it.
2. **Private, in `private/`**: coursework results and exports from the reference software. The matching tests sit behind the Cargo feature `private-oracle` and are skipped when the folder is empty. No file, number or subject name from `private/` may appear in a versioned file, a commit message or the board.
3. **Independent computation**: for a case with no reference, a second, naive implementation in `caladrius-testkit` (never the same function tested against itself).

Tolerances (defined once in `caladrius-testkit`, never loosened inside a test):

- NCA against PKNCA: relative error ≤ 1e-6.
- Against a reference export: equality at the precision displayed in the export.
- Fitting: parameters within 1e-4 relative, weighted sum of squares within 1e-6 relative. Two optimizers do not reach the same minimum bit for bit; a gap beyond tolerance is a method difference to document in `specs/differences.md`, not a tolerance to widen.

"100% reproduction" means: 100% of in-scope parameters, within these tolerances, on every oracle case. `cargo xtask conformance` generates `docs/conformance.md` (validated parameters per case); never edit that file by hand.

## 6. Reference documentation: how to use it

Goal: learn the conventions and defaults that change a number (point selection for λz, trapezoid rule, handling of values below the limit of quantification, of zero times, of extrapolation, definition of the weights, stopping criterion of the fit).

Two separate roles, to keep the clean room credible:

- The **reader** agent is the only one who reads the public documentation of the reference software. It writes in `specs/` a description of the behaviour **in its own words and in formulas**, with no copied sentence, citing for each rule its source (page title) and, where possible, a second independent source (textbook, paper, PKNCA).
- **Implementer** agents read only `specs/` and the oracle. They never open the reference documentation.

Every rule in `specs/` carries a status: `confirmed by oracle`, `documented, untested` or `assumed`. An `assumed` rule is never presented as settled; it becomes a test task.

Fitting defaults seen on the human's screen (status `assumed` until the reader confirms them and the oracle tests them): Gauss-Newton minimization with the Levenberg and Hartley modification, increment for partial derivatives 0.001, convergence criterion 0.0001, 50 iterations maximum, 1000 predicted values for the curve, uniform weighting on observed values, parameter bounds generated by the software unless the user supplies them. The same method must be available in `caladrius-fit` as the named preset `reference_conventions`; since Q-014 (2026-10-09, option 2) the default is the exact minimum (closed-form derivatives, relative decrease 1e-10), documented as D-04 in `specs/differences.md`. Another optimizer may exist as an option, never as a silent replacement.

If the reference software's license or the documentation site's terms forbid this use, the reader stops and records it in `board/QUESTIONS.md`.

## 7. UI and screenshots

Design goal: a user of the reference software finds the same way of working (project tree, worksheet, analysis object with setup and results, plots) and recognizes nothing else. Caladrius must be visibly more modern, faster and easier: fewer clicks, sensible defaults, live preview, readable errors.

- Screenshots in `private/screenshots/` are seen by the **reader** only. It writes `specs/ux.md`: the workflow step by step, what each screen lets the user decide, and a list of friction points to remove. It describes; it does not transcribe labels or layouts.
- The **interface** agent works from `specs/ux.md` and the theme tokens. It never opens the screenshots.
- Do not reproduce the reference software's layout pixel for pixel, its icons, its colours or the wording of its dialogs. Standard domain vocabulary (NCA, AUC, λz, Cmax…) is free to use.
- Required improvements for v1: click points on the curve to choose the λz range; the predicted curve updates live while initial estimates change; one screen from data to results for NCA; every error says what to fix.
- Friction observed in the reference workflow, each to be removed:
  1. Creating a model takes a four-level nested menu from a worksheet. Caladrius: one "New analysis" button and a command palette.
  2. Settings for one analysis are spread over a left-hand list, five bottom tabs and a separate setup/results switch. Caladrius: one page, read top to bottom (data, model, dose, initial estimates, options), with the plot always visible.
  3. Results are a flat tree of about twenty tables and six plots opened one at a time. Caladrius: a summary first (parameters with precision, fit plot, residuals, goodness of fit), details on demand.
  4. A poor fit is not flagged: a CV% in the hundreds or a confidence interval crossing zero looks like any other number. Caladrius: flag them and say what to check.
  5. Units are free text and inconsistent combinations pass silently into derived units. Caladrius: check dose, concentration and time units together and warn before running.
  6. Models are picked from a numbered table. Caladrius: pick by route and number of compartments, with the diagram and equation shown.
  7. The project tree fills with objects named "PK 1", "ASCII 2". Caladrius: name objects from their content and show stale results clearly after an input change.
  8. User models need a fixed-format block language. Caladrius (later step): readable equations with named parameters and immediate error messages.
- No screenshot of the reference software ever enters the repository, the README or an issue.

## 8. Agent organization

| Role | Model | Does | Does not |
|---|---|---|---|
| `orchestrator` | Fable (else Opus) | splits, assigns, reads reports, keeps the board, decides | write code, read large files |
| `engine` | Opus | `caladrius-nca`, `caladrius-models`, `caladrius-fit`: everything that produces a number | UI |
| `reader` | Sonnet | `specs/` from the allowed sources and the screenshots | code |
| `oracle` | Sonnet | `oracle/`, `caladrius-testkit`, R scripts, tests that fail first | fix the engine to make a test pass |
| `interface` | Sonnet | `caladrius-project`, `caladrius-engine` commands, `caladrius-ui`, `caladrius-cli`, `caladrius-mcp` | numerical code |
| `reviewer` | Sonnet | reviews a diff it did not see being written; hunts panic paths and bypassed tolerances | modify the reviewed code |
| `janitor` | Haiku | formatting, renames, file moves, board housekeeping asked by the orchestrator | any decision |

Definitions live in `.claude/agents/<role>.md`, with the model in the front matter. The orchestrator creates them at step 0.

Rules:

- A subagent receives **one** task from the board, with its allowed files and its completion criterion. It returns a report of at most ten lines: what was done, the numbers, what is still open.
- Whoever writes an oracle test does not write the code under test.
- Every change in `caladrius-nca`, `caladrius-models` or `caladrius-fit` goes through the `reviewer` before being marked done.
- Parallel work: two agents never touch the same crate. Each uses `CARGO_TARGET_DIR=target/agent-<role>`. A new module per feature instead of growing a shared file. If someone else's work breaks the build, wait and retry; do not fix their files.

## 9. The shared board

```text
board/
  INDEX.md          overview: one line per task (id, title, role, state). Only the orchestrator writes it.
  tasks/T-042.md    one card per task. Only the assignee writes it, plus the orchestrator.
  LOG.md            append-only: one entry per finished task.
  QUESTIONS.md      questions for the human, with the decision taken meanwhile.
  messages/         one file per message between agents: <date>-<from>-<to>-<subject>.md
```

Task card: goal, allowed files, verifiable completion criterion (a command and its expected result), dependencies, state (`todo`, `in progress`, `to review`, `done`, `blocked`), then a dated thread of notes.

One file per task and per message: this is what stops two agents from editing the same file. Subagents do not talk to each other directly; they write in `messages/` and the orchestrator relays. The board is versioned: it is also the project's memory from one session to the next.

## 10. Budget: subscription constraints

The project runs on a Max subscription, with no pay-as-you-go billing. The quota refills in windows; when it runs out, the session stops dead, with no warning. Consequences:

1. **Always resumable.** After each task: tree builds, tests green, commit, entry in `LOG.md`. A session can be cut at any moment; the next one restarts from the board, not from conversation memory.
2. **Sequential by default.** At most 2 subagents in parallel, and only on independent crates. Never 8.
3. **The most expensive model reads the least.** The orchestrator reads the board and the reports, not the code. Opus is reserved for the `engine` role. File search, renames, formatting: Sonnet or lighter.
4. **Small tasks.** A task fits in one subagent's context: one function and its tests, not a whole crate.
5. **No loops.** Three failed attempts on the same test: the task becomes `blocked` with the diagnosis, and you switch tasks.
6. **Do not re-read needlessly.** Read the parts of a file you need, not the whole file; do not rerun the whole test suite when one crate is enough.
7. **No speculative work.** Nothing outside section 1, no rewrite "while we're at it".
8. **No paid service.** No billed API, no paid CI, no dependency under a commercial license.

## 11. Before you finish a task

```sh
cargo test -p <crates you touched>
cargo clippy -p <crates you touched> --all-targets -- -D warnings
cargo xtask layers
cargo xtask wasm          # if L0 to L3 changed (advisory until step 6)
cargo xtask conformance   # if a number changed; commit docs/conformance.md
```

Then: task card updated, entry in `LOG.md`, commit whose message starts with the task id.

Forbidden without written agreement in `QUESTIONS.md`: `git push`, changing the repository's visibility, deleting files outside the repository, installing software outside package managers (cargo, rustup, R), touching `private/` other than reading it.

## 12. Marching order

0. Skeleton: workspace, empty crates, `xtask layers`, agent definitions, board, `.gitignore` with `private/`.
1. `specs/` for NCA, public `oracle/`, tests **that fail**. Nothing else.
2. `caladrius-nca` up to 100% of the public oracle, then of the private oracle if present.
3. `caladrius-models` and `caladrius-fit`: one-compartment oral, then zero-order absorption and infusion.
4. `caladrius-cli` and `caladrius-mcp`: a CSV in, result tables out; the same commands over MCP.
5. `specs/ux.md`, then the UI: worksheet, NCA with λz point selection on the plot, fitting with the live curve, observed/predicted and residual plots.
6. WebAssembly demo and README with the conformance table.
7. Only then: two compartments, user-written models, several subjects.

Move to the next step only when the previous step's criterion is green in `docs/conformance.md`.
