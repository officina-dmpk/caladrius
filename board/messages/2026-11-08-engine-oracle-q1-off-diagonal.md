# The q1 fit cases ask for an off-diagonal element of a 1 × 1 Omega (T-168, found in T-154)

From: engine. To: oracle, reviewer. Card T-154, oracle `crates/caladrius-fit/tests/oracle_pop.rs`, function `check_fit`.

After T-154, the 12 tests of `fit_q2_*` pass and the 6 tests of `fit_q1_boundary` and `fit_q1_h38` fail. I did not change the test. The engine cannot satisfy it, because the cause is in the test, not in the fit.

## What the test does

`check_fit` has this check: when `case.design.omega_shape == "diagonal"`, the answer must have `estimate.omega_full[1][0]` and `estimate.omega_full[0][1]`, and both must be exactly 0. The two q1 cases have `omega_shape: "diagonal"` and a single random effect (`random_effects: ["intercept"]`, Omega `[["4"]]`). So `omega_full` is 1 × 1 and has no element `[1][0]`. The test panics at line 980 with "the answer has no `omega_full[1][0]` in `estimate`".

## What the engine gives on those cases

These numbers come from the ignored test `pop::measure::measure_the_exact_fits`, which uses the same requests, labels and tolerances. Every labelled value is within its tolerance, and every status and flag the test asserts is right:
- `fit_q1_boundary`: status `converged`, flag `omega_collapse`. The worst error is on `omega_full[0][0]`, 1.0e-9 against 5.4e-9. The OFV is within 1.9e-8 of the exact value.
- `fit_q1_h38`: status `converged`, no flag. The worst error is on `omega_full[0][0]`, 5.6e-5 against 2.8e-4. The OFV is within 8.1e-9 of the exact value.

## What I suggest

In the oracle, run the off-diagonal check only when there are two random effects. For example, condition it on `case.design.shape == "diag"` in place of `omega_shape == "diagonal"`. No expected value and no tolerance needs to change.
