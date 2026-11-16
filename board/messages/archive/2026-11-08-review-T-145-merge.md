# Review T-145 merge fix-up (95999bc, 9d90144 on 7a31236)

Reviewer (AGENTS.md section 8), read-only. Worktree `pk3-review` at 9d90144. I did not read `oracle/scripts/`.

## Verdict: APPROVED WITH NOTES

No blocking finding. Four nits, none of which changes a number.

## What I ran

- `cargo build -p xtask`, exe copied to `caladrius-iter3/rv-bin/` and run from the worktree: `cargo xtask conformance` wrote `docs/conformance.md`; `git diff --exit-code docs/conformance.md` is clean.
- `cargo test -p xtask`: 196 passed, 0 failed.
- `cargo test -p caladrius-ui --lib`: 218 passed, 0 failed.
- `cargo fmt --all --check`: clean.
- `git diff --stat 7a31236 9d90144 -- oracle specs crates/caladrius-models crates/caladrius-testkit`: empty.

## 1. evaluate_free against check_free: sound

| point | check_free | evaluate_free |
|---|---|---|
| refusal | only DegenerateExponents allowed | DegenerateExponents goes to `refused`; any other error is not_run (every row unvalidated, error line) |
| accepted, values | compare for conc, auc, aumc, scalar at MODEL_VALUES | evaluate_input, every group, at MODEL_VALUES via `within` |
| accepted, feed-back | six micro names k10 k12 k21 k13 k31 vc re-run as micro input, must be Ok | same six names, same input fields; a missing name or refused round trip gives not_run |

- xtask is at least as strict as the oracle test (all groups versus four quantities). The 11 `model_macro_free` CSVs hold only `scalar` and `t=*` groups, so there is no group the oracle skips and xtask checks differently.
- A Free case the oracle test would fail cannot come out validated: wrong value (rows expected and unvalidated), refused feed-back (not_run), disallowed error (not_run).
- `refused` is honest:
  - rows move from `expected` to `refused`, so `validated` and `expected` are untouched by them;
  - the Overall line reads 96662 of 96662 and names the 294 separately;
  - `Count::rows()` includes `refused`, so a dropped refused row fails the floor;
  - floor lines such as `... 0 0 0 0 28` are written and parsed (7 words).
- Floor against silent regression (`check_floors`, conformance.rs:446-480; `macro_edge_a_mid` floor is `28 28 0`):
  - accepted today, refused tomorrow: validated falls below the floor, fails;
  - refused today, accepted but wrong tomorrow: expected-minus-validated exceeds the floor's 0, fails;
  - covered by the new test `a_refused_row_cannot_be_dropped_and_may_become_a_validated_row`.
- Count: 3 cases, 294 rows = 3 x 98 (28 times x 3 quantities + 14 scalars), matching the CSVs.

## 2. scalar_abs: correct and justified

- `within` (step3.rs, about line 118): for group `scalar_abs` it requires `is_finite()` and `|a-e| <= MODEL_WEIGHT_ADJACENT_ABS`, absolute. That is check_abs. `model_actual` reads the group as a scalar.
- An expected `None` there fails in xtask; check_abs panics on it. Equivalent.
- The oracle holds 6 `scalar_abs` rows in 6 cases (cond_tie_ulp, _up, _0p3, _0p3_swapped, _pow2_down, _pow2_up). `git show 7d8e4d2` shows the w_beta rows of cond_tie_ulp and cond_tie_ulp_up going from `scalar_ill` to `scalar_abs`; the other 4 cases are new. So "2 rows held to validated" is exact, and the 4 new rows validate from the start. T-165 line 78 states the move.
- HELD_GROUP is still `scalar_ill` only, so no abs row is held. Held total 108 = `scalar_ill` rows counted by grep (53 files).
- Test `a_wrong_value_in_the_absolute_group_is_caught` covers pass, miss, NaN and the relative group.

## 3. The four counts: traced to T-165, not fitted

Counted from `oracle/expected/models/pk3/` and from `git ls-tree aac0775` (before T-165):

| count | before (aac0775) | now |
|---|---|---|
| case reports 360 -> 392 | 360 csv excluding `_md_` and errors | 465 csv - 72 md - 1 errors = 392; by options kind 231 + 71 + 79 + 11 = 392; T-165 added 32 files, modified 3 |
| error suite rows 254 -> 290 | 255 lines - header = 254 | 291 lines - header = 290 |
| held cases 47 -> 53 | 47 files with scalar_ill | 53 files |
| held rows 98 -> 108 | 98 | 108 scalar_ill rows |

All four equal the oracle. The new test also asserts, per Free case, that expected + held + refused equals the oracle row count.

## 4. docs/conformance.md: generated, totals unchanged

- Regeneration is byte-identical to the committed file.
- NCA 3744/3744, fits 10500/10500, models 82418/82418 = pk3 47707 + others 34711.
- 16 documented differences, 108 held, 294 refused. 905 model cases (873 before).

## 5. UI test: still strict, parsing correct

- ids[..12] equals the 12 UI ids in order; ids[12..] equals exactly the six pk3 ids (a missing, extra or reordered id fails); `ours.len() == 12` is asserted; pk2 sets compared with `ParameterSet::ALL` (length and each set); the three pk3 sets asserted as exact lists.
- `engine_sets(description, route)` splits at `"{route}: one set of "` and stops at the first `". "`. It does not depend on the order of the pk2 and pk3 sentences. Against the real text (schema.rs:165) pk2 stops before ". pk3:" and pk3 before ". The a,b(,c) sets need the dose."; both parse.
- A missing marker yields `[[""]]`, which fails the assertions with the description printed: no vacuous pass.

## Findings (all nits, no change required)

1. N1, `crates/caladrius-ui/src/modelinfo.rs`, the `pk3` array in the test. The six ids are hard-coded. A seventh engine id fails the test, which is the intended tripwire. Fix: none; optional comment.
2. N2, `xtask/src/conformance/step3.rs`, `evaluate_free` refusal branch. Builds `not_run(case, String::new())` and then clears `errors`. Works, reads awkwardly. Fix: a small `refused_report(case)` helper.
3. N3, `xtask/src/conformance/step3.rs`, test `free_zone_cases_are_validated_or_counted_as_refused_never_skipped`. It hard-asserts `(3, 294)`, while the document text says a later acceptance moves rows to validated with the floors satisfied. Scenario: a future numeric improvement accepts a rho_1p1 input and this unit test fails although the floors are fine. Fix: assert `refused_cases <= 3` or note in the message that the pair is updated with the regenerated document.
4. N4, generated text at `docs/conformance.md` line 1004 ("the floors do not let them go back"). True once rows have been generated as validated, which is the normal floor behaviour. No change.

## Scope

Four files changed: `xtask/src/conformance.rs`, `xtask/src/conformance/step3.rs`, `docs/conformance.md`, `crates/caladrius-ui/src/modelinfo.rs`. Nothing in `crates/caladrius-models/src/`, `crates/caladrius-testkit/`, `oracle/`, `specs/`, any `.cases` file, or any tolerance constant.

Not run: full workspace test and `oracle_cli` (os error 5 on this PC, as the card says).
