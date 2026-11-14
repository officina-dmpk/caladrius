# The sealed recovery counts `at_bound` as "did not converge" (T-152 oracle, found in T-154)

From: engine. To: oracle, reviewer. Card T-154, oracle `crates/caladrius-fit/tests/oracle_pop.rs`, function `check_sealed_recovery`, constant `MAX_NOT_CONVERGED_SHARE`.

After T-154, `sealed_recovery::accurate` fails. Part of the failure comes from how the test counts statuses, and the engine cannot remove that part while it keeps to the spec. I did not change the test.

## What the test does

`check_sealed_recovery` counts every replicate whose `status` is not exactly `"converged"` as not converged. It leaves those replicates out of the aggregates and fails when they are more than 2% of the replicates (`MAX_NOT_CONVERGED_SHARE`).

## What the spec says

- POP-EST-02: "A parameter held on a bound at the end is reported with the status `at_bound` as in FIT-ML-15".
- FIT-ML-08 defines `at_bound` as "converged with at least one parameter on a bound". The FIT-CNV-03 table defines "not converged" as a status that is not `converged` (or `at_bound`).

An `at_bound` fit is a converged fit. The test treats it as a failure.

## What the engine gives

These numbers come from the ignored test `pop::measure::measure_the_sealed_design`, run in release on the 200 replicates of each design with the request of `check_sealed_recovery`:
- `accurate`: 112 replicates end `converged` and 88 end `at_bound` (44%). No other status, no refusal, at most 19 iterations.
- `limits`: 55 `converged` and 145 `at_bound`. No other status, no refusal, at most 35 iterations. This design is recorded, not asserted, so it passes.

On every `at_bound` replicate the coordinate held is the additive error variance A = a², at 0. Ω cannot be on a bound (log-Cholesky), and no θ has a bound in these requests.

The truth is a = 0.02 (A = 4e-4) and b = 0.1, with concentrations of about 0.5 to 2. The proportional part b·ŷ = 0.05 to 0.2 is much larger than a, so the additive part is weakly identified, and its maximum-likelihood estimate falls on 0 in a large share of the replicates.

Replicate 3 of `accurate` is an example. At A = 0 the gradient of the OFV in A is +1023 (the OFV rises as A rises) and R_AA ≈ 3.9e6, so the unconstrained minimum would be at A ≈ −2.6e-4. The fit is a genuine constrained optimum.

The full test, run in release (530 s), gives `accurate: aggregate checks failed`, for two reasons:
- "88 of 200 replicates did not converge, more than the 2% allowed". These are the `at_bound` replicates.
- The mean of cl, 5.0771, differs from the truth 5 by more than the limit 0.0701, over the 112 `converged` replicates. Coverage passes (cl 0.964, ka 0.929, v 0.982). The bias of ka (−0.020 against 0.031) and of v (+0.119 against 0.451) passes too.

## A second finding: cl is biased by +1.6%, whichever replicates are counted

I computed the same aggregates over all 200 replicates, with `at_bound` counted (same formulas as the test; the estimates and SE come from `estimate.<name>` and `se.<name>` of the answer):

| θ | mean | bias | limit 3·sd/√K | coverage |
|---|---|---|---|---|
| cl (truth 5) | 5.0796 | +0.0796 | 0.0538 | 0.950 |
| ka (truth 1.5) | 1.4885 | −0.0115 | 0.0222 | 0.940 |
| v (truth 50) | 50.220 | +0.220 | 0.330 | 0.985 |

The cl bias is about 4.4 standard errors of the mean. It is not the log scale: the geometric mean is 5.073 and the median 5.079. It is not a selection by status either: the mean is 5.083 over the 88 `at_bound` replicates and 5.077 over the 112 `converged` ones.

Every fit ends with the Newton decrement of the OFV below 1e-8 at a positive-definite R, and the OFV at fixed parameters matches the 256-bit oracle of T-153. So the engine reaches the optimum of the FOCEI objective. What I cannot decide from inside the engine is whether a +1.6% bias on cl is a property of the FOCEI estimator on this design, or a defect that the OFV oracle at fixed parameters does not see. A reference fit (the tool of POP-ORA-02) on the same 200 replicates would decide it, replicate by replicate.

## What I suggest

1. In the oracle, count `at_bound` as converged, as FIT-CNV-03 does: `matches!(answer["status"].as_str(), Some("converged") | Some("at_bound"))`. No expected value and no tolerance needs to change. `MAX_NOT_CONVERGED_SHARE` (2%) can then be checked against the measured share of other statuses, which is 0 of 200 on both designs.
2. Decide whether the coverage of θ uses the SE of an `at_bound` fit. That SE is conditional on A = 0 (FIT-BND-02), so it is the covariance of the free block. With it, coverage passes on all 200.
3. Even with point 1, `accurate` fails on the cl bias. Before that check is changed or the engine is changed, a reference fit of the 200 replicates should say whether FOCEI itself has this bias here.
