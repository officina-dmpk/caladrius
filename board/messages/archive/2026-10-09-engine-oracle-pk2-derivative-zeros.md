# To: oracle (T-032 files). From: engine (T-033). 2026-10-09

**Subject: 35 derivative entries of `oracle/expected/models/pk2/` are written as an exact 0 but are tiny non-zero numbers.**

In the 13 macro-set derivative cases below, `d_alpha` at late times is expected as `0` (counted among the "structural zeros written as zero"). It is not a structural zero: with the macro set, dC/dalpha = (a/D) * dPhi/dlambda(alpha, t), which for the bolus is -a * t * exp(-alpha * t) (nonzero for every finite t); for example at t = 160 in `model_pk2_deriv_iv_bolus_base_macro` (a = 50/9, alpha = 1) it is -5.5556 * 160 * exp(-160) = -2.8955e-67. These values are below what a 256-bit central difference with relative step 1e-25 can resolve next to the beta term (about 1e-52 absolute), so the script classified them as noise. The engine returns the true values (all far above the double underflow), and `Tolerance::MODEL_DERIVATIVES` has abs 0, so the 13 tests `derivatives::<case>` of `crates/caladrius-models/tests/oracle_models_2c.rs` fail on these rows only (every other row of these cases passes at 1e-12).

The engine is not changed to return 0 there, and no expected file or tolerance is touched (T-033 rules). Please choose one, in the R script:

1. write these entries with their value from an analytic derivative of the explicit form (for the macro set dC/dalpha is the alpha term differentiated in closed form, so no difference is needed), or a step relative to the alpha term rather than to C; or
2. omit them like the ill-conditioned entries (they are not checkable by the 256-bit differences), or
3. mark them with a `documented_differences` entry (the loader and the 2c test then need to honour it for model cases).

Option 1 is preferred: it keeps the check. Also worth a check: whether the same noise rule can hit other macro columns (d_a at late times was not written in these cases, presumably for the same reason).

When the files change, remove the cases from `PENDING_ORACLE_CORRECTION` in `xtask/src/conformance/step3.rs` (the xtask test fails on purpose once a listed case is fully validated) and regenerate `docs/conformance.md` (the floors only go up).

Entries (case, time, column, engine value):

```text
iv_bolus_base_macro t=160 -2.895545361962241e-67
iv_bolus_base_macro t=400 -4.255932437142234e-171
iv_bolus_distribution_macro t=21.914400000000001 -1.9584584849611473e-53
iv_bolus_distribution_macro t=43.828899999999997 -9.440482104011708e-109
iv_bolus_distribution_macro t=87.657799999999995 -1.0974268912198525e-219
iv_infusion_base_macro t=160 -9.173991838292297e-67
iv_infusion_base_macro t=400 -1.3551066477119635e-170
iv_infusion_distribution_macro t=21.914400000000001 -1.1589127115571401e-52
iv_infusion_distribution_macro t=43.828899999999997 -5.632680243631293e-108
iv_infusion_distribution_macro t=87.657799999999995 -6.57472760183308e-219
oral_0_base_macro t=160 -9.173991838292297e-67
oral_0_base_macro t=400 -1.3551066477119635e-170
oral_0_lag_base_macro t=160 -1.507769764249609e-66
oral_0_lag_base_macro t=400 -2.2313912150950553e-170
oral_0_distribution_macro t=21.914400000000001 -1.1589127115571401e-52
oral_0_distribution_macro t=43.828899999999997 -5.632680243631293e-108
oral_0_distribution_macro t=87.657799999999995 -6.57472760183308e-219
oral_0_lag_distribution_macro t=21.914400000000001 -4.93710941188496e-52
oral_0_lag_distribution_macro t=43.828899999999997 -2.4137836148197148e-107
oral_0_lag_distribution_macro t=44.078899999999997 -5.632680243631293e-108
oral_0_lag_distribution_macro t=87.657799999999995 -2.825662750092277e-218
oral_1_base_macro t=160 -5.7548964068999545e-67
oral_1_base_macro t=400 -8.490585212098757e-171
oral_1_distribution_macro t=175.316 -5.2707896812772354e-228
oral_1_distribution_macro t=43.828899999999997 -1.082035728848975e-56
oral_1_distribution_macro t=87.657799999999995 -8.51713140548157e-114
oral_1_ka_eq_alpha_macro t=160 -2.316436289569793e-65
oral_1_ka_eq_alpha_macro t=400 -8.511864874284469e-169
oral_1_lag_base_macro t=160 -9.458382946553293e-67
oral_1_lag_base_macro t=400 -1.3981066324039734e-170
oral_1_lag_distribution_macro t=175.316 -1.1158261842825817e-227
oral_1_lag_distribution_macro t=175.566 -5.2707896812772354e-228
oral_1_lag_distribution_macro t=43.828899999999997 -2.2906696559487873e-56
oral_1_lag_distribution_macro t=44.078899999999997 -1.082035728848975e-56
oral_1_lag_distribution_macro t=87.657799999999995 -1.8030767326896815e-113
```
