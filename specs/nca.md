# Non-compartmental analysis (NCA): behaviour specification

Card T-002. Written by the `reader` agent on 2026-10-08, in its own words and formulas. Sources are cited by the keys of `specs/sources.md` (S-01 …); `H` means a hand check by the reader, `D` a derivation shown here.

## 0. How to read this file

**Status tags.** Every rule carries one: `confirmed by oracle`, `documented, untested`, or `assumed` (see `specs/README.md`). At this step no versioned oracle test exists, so nothing is `confirmed by oracle`. An `assumed` rule is a test task, never a settled fact.

**Rule ids.** `NCA-<AREA>-<nn>`. Tests and commits should cite them. The areas are DAT (data and cleaning), OBS (observed parameters and dose normalisation), AUC, LZ (λz), EXT (extrapolated and derived parameters), IV (intravenous bolus), UNIT (units), OUT (what a result looks like). A rule that mixes a documented part and an assumed part is split in two ids (suffix `b` for the assumed part), so that the counts of tags are honest.

**The reference software's own documentation was not used** (`specs/sources.md` section 2, question Q-005). Where this file speaks of the reference software, the facts come from a conference poster, a peer-reviewed paper, and the public documentation of other NCA tools. Where those do not settle a convention, the rule says so and is tagged `assumed`.

**Two profiles of options.** Most conventions are options (golden rule 5). This file gives two sets of values:

- the **PKNCA profile**: the defaults of PKNCA 0.12.1, documented in S-01, S-02, S-03. The public oracle (T-003) is generated with it; the R script must write every option explicitly and record the PKNCA and R versions.
- the **reference profile**: what independent sources say about the reference software. Mostly `assumed`. The private oracle will settle it by observed results.

Which profile is the application default is the orchestrator's decision, after question Q-008 is answered.

**"Not calculated" (NC)** means the result is absent and carries a machine-readable reason. It is never NaN, never an infinity, and never a zero used as a stand-in.

**Scope.** One profile (one subject, one analyte, one occasion), single dose, closed-form NCA. Section 12 lists what is deliberately not specified.

**Numerics.** Double precision. No rounding inside the computation. Public oracle tolerance: relative error ≤ 1e-6 against PKNCA (`AGENTS.md` section 5). Regression sums should be computed on centred data.

## 1. Notation

| symbol | meaning |
|---|---|
| (t_i, C_i) | observation i of the cleaned profile, i = 1 … N, times strictly increasing |
| t_d | dose time, default 0; all result times are relative to it |
| D | dose (amount of the dose unit; may itself be per body weight) |
| T_inf | infusion duration; 0 for bolus and extravascular |
| Tmax, Cmax | time and value of the first maximum |
| Tlast, Clast | last time with C > 0, and C there |
| λz, a | terminal rate constant and intercept: ln C ≈ a − λz·t on the chosen points |
| Clast,pred | exp(a − λz·Tlast) |
| AUC_seg(i), AUMC_seg(i) | area under C and under t·C between points i and i+1 |

## 2. Contract (language-neutral)

The exact Rust signature belongs to the engine agent. The oracle agent writes tests against the field names and semantics below; T-003 may transcribe them into its card.

### 2.1 Input

| field | meaning |
|---|---|
| `time[]` | numbers, in the time unit |
| `conc[]` | numbers, or missing |
| `blq[]` (optional) | per-point flag: this point is below the limit of quantification (NCA-DAT-05) |
| `lambda_z_exclude[]` (optional) | per-point flag: leave this point out of automatic λz selection |
| `lambda_z_points` (optional) | manual λz selection: explicit per-point flags, or an inclusive time range |
| `route` | `extravascular`, `iv_bolus` or `iv_infusion` |
| `infusion_duration` | required and > 0 iff `iv_infusion` |
| `dose` (optional) | amount and unit; `dose_time` defaults to 0 |
| `units` | time unit, concentration unit; optional molecular weight (NCA-UNIT-02) |
| `options` | section 2.2 |

### 2.2 Options and their two profiles

| option | PKNCA profile (public oracle) | reference profile |
|---|---|---|
| `auc_method` | `lin_up_log_down` (also `linear`, `lin_log`) | unknown; settled by Q-008 (`assumed`) |
| `missing_policy` | drop | drop (`assumed`) |
| `blq_policy` | first keep, middle drop, last keep | unknown; settled by Q-008 (`assumed`) |
| `negative_policy` | not applicable (PKNCA warns) | `error` by default, `allow`, `set_zero` |
| `start_policy` | `none` | `auto` (`assumed`, NCA-DAT-08b) |
| `tmax_tie` | first | first |
| `lambda_z_min_points` | 3 | 3 |
| `lambda_z_allow_tmax` | false | false for extravascular and infusion; true for IV bolus (`assumed`, NCA-LZ-14) |
| `lambda_z_tolerance` | 1e-4 | 1e-4 |
| `lambda_z_tie_rule` | `tolerance` (reading settled by T-003, NCA-LZ-06) | `tolerance` |
| `lambda_z_positive_filter_first` | false | true |
| `c0_methods` | by route (NCA-IV-01) | by route (`assumed`) |
| quality thresholds (flags only) | adjusted R² 0.9, span ratio 2, extrapolated AUC 20 % | unknown (`assumed`) |

### 2.3 Output

- one entry per parameter of section 10: `id`, `value`, `unit`, and either `ok` or `not_calculated(reason)`;
- `lambda_z_candidates[]`: every candidate fit (number of points, first and last time, λz, R², adjusted R², whether valid, whether selected). Always returned, so a UI can show the choice and let the user change it (NCA-OUT-01);
- `profile_used[]`: the cleaned points actually integrated, with the reason for every removal or change;
- `warnings[]`.

### NCA-OUT-01 Candidate fits are always returned
The full table of λz candidates (valid or not, selected or not) accompanies every result, also when λz is NC or manually selected.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 (the user must be able to click points on the curve to choose the λz range; errors say what to fix).

### NCA-OUT-02 Absence is explicit
A result that cannot be computed is NC with a reason from a closed list; it is never NaN, an infinity or a stand-in zero. A result that is computed but doubtful carries flags (NCA-LZ-12b) instead of being removed.
- Status: `documented, untested`
- Sources: S-03 (0.8.2: infinity from division by zero replaced by NA; 0.9.0 and 0.12.0: exclusion reasons recorded); S-02:v31 (a missing result or an error is preferred to a doubtful number).

### 2.4 Error codes

Hard errors stop the run before any computation: `LengthMismatch`, `EmptyProfile`, `NonFiniteTime`, `DuplicateTime`, `UnsortedTime`, `InfiniteConcentration`, `NegativeConcentration` (under the default policy), `InvalidDose`, `InvalidRoute`, `InvalidInfusionDuration`. Everything else produces NC reasons, listed in section 9.

## 3. Data and cleaning

### NCA-DAT-01 Time origin
One analysis is one profile. Integration starts at t_d. Every time-valued result (Tmax, Tlast, Tlag, λz window times) is relative to t_d.
- Status: `documented, untested`
- Sources: S-01:pk.nca (times are relative to the interval start); S-02:v01 (data outside the group and interval are never looked at).

### NCA-DAT-02 Structural validity
Stop with a readable error when: the two arrays differ in length; the profile is empty; a time is missing, NaN or infinite; times are not strictly increasing (this covers both unsorted and duplicated times); a concentration is infinite. Never sort or merge silently; the data layer may offer sorting or merging on an explicit user request.
- Status: `documented, untested`
- Sources: S-01:assert_conc_time (invalid when a time is not a number or missing, times not monotonic, lengths differ; the sorted-time check requires unique increasing times); S-02:v31 (a missing result or an error is preferred to a doubtful number).

### NCA-DAT-03 Missing concentrations
A missing or NaN concentration is removed together with its time before any other step. An option may replace it by a given number instead.
- Status: `documented, untested`
- Sources: S-01:clean.conc.na; S-02:v40 (default drop); S-02:v01 (missing is coded as NA).

### NCA-DAT-04 Negative concentrations
Default policy: stop with `NegativeConcentration`, telling the user to mark the point as BLQ or choose a policy. With `allow`: negative values enter AUC and AUMC as numbers using the linear rule (the log rule needs both ends > 0); they never enter the λz regression; they are never Tlast or Clast (those need C > 0). With `set_zero`: they become zeros and follow the BLQ policy.
PKNCA only warns and continues, and its rule text for lin-up/log-down applies the log rule whenever the later value is non-zero, which is undefined for a negative value; the guard "later value > 0" is the reader's choice.
- Status: `assumed`
- Sources: S-01:assert_conc_time (warning only); S-02:v23; S-09 (another tool keeps negative values in data processing and drops them only from the λz regression).

### NCA-DAT-05 BLQ recognition
A concentration equal to 0 is a BLQ point.
- Status: `documented, untested`
- Sources: S-01:clean.conc.blq (values are BLQ if they are 0); S-02:v01.

### NCA-DAT-05b Explicit BLQ flag
An optional per-point flag also marks a point as BLQ when its stored value is non-zero (for example a value already substituted by the data layer). A flagged point is treated as 0 before the policy of NCA-DAT-06 is applied.
- Status: `assumed`
- Sources: extension of NCA-DAT-05, needed because the CSV import may carry a BLQ column; S-09 (another tool reads a censoring flag).

### NCA-DAT-06 BLQ policy
BLQ points are classified, after missing values have been removed, either by position relative to the quantifiable points (C > 0) or by position relative to Tmax:
- `first`: BLQ points before the first quantifiable point (all points if none is quantifiable);
- `middle`: between the first and the last quantifiable point;
- `last`: after the last quantifiable point;
- or `before_tmax` / `after_tmax`: before or after the time of the first maximum.

Each class gets one action: `drop` (remove the point and its time), `keep` (leave the zero), or `set(x)` (replace the value by the number x, for example half the LLOQ). A single action may be given for all classes. PKNCA default: first keep, middle drop, last keep.
- Status: `documented, untested`
- Sources: S-01:clean.conc.blq; S-02:v40; S-03 (0.12.0 added the Tmax-relative classes); S-09 (another tool offers zero, LLOQ, LLOQ/2 or missing).

### NCA-DAT-07 Where the policy acts
The policy is applied once, before Cmax, Tmax, AUC, AUMC and λz. Dropping a point makes its neighbours adjacent for integration (the segment rule is applied across the gap). Tlast and Clast do not depend on the policy (they depend only on C > 0).
- Status: `documented, untested`
- Sources: S-02:v05 (AUClast ends at Tlast; AUCall uses the trailing zero); S-01:clean.conc.blq.

### NCA-DAT-08 Zero-time and start of the profile (`start_policy`)
The AUC of a profile needs a concentration at the start time t_d. PKNCA does not invent one: if the first sample is after t_d, an AUC that starts at t_d is not calculated unless an imputation is requested (insert 0 at t_d; or move a pre-dose sample to t_d; or insert the minimum). Caladrius offers `none` (PKNCA behaviour, reason `no_start_concentration`), `zero` (insert (t_d, 0)), `c0` (insert (t_d, C0), IV bolus, NCA-IV-01), and `auto` (see NCA-DAT-08b). An observation at t_d is always used as it is.
- Status: `documented, untested`
- Sources: S-02:v08 (an AUC range starting before the first measurement is refused; imputation is opt-in; the three imputation methods).

### NCA-DAT-08b `auto` start policy as the reference behaviour (hypothesis)
`auto` inserts (t_d, 0) for extravascular and infusion data and (t_d, C0) for IV bolus data whenever there is no observation at t_d. Another tool does this without being asked; whether the reference software does is unknown.
- Status: `assumed`
- Sources: S-08 and S-09 (zero added for extravascular and infusion single dose, back-extrapolated C0 for bolus).

### NCA-DAT-09 Degenerate profiles
- One remaining point: AUC, AUMC and everything derived from them are NC.
- All concentrations zero (two or more points): AUC and AUMC are 0, Cmax is 0, Clast is 0; Tmax, Tlast and λz are NC.
- No point left after cleaning: everything NC.
- Status: `documented, untested`
- Sources: S-03 (0.10.0: a single point gives NA, not 0; 0.11.0: Clast is 0 when all are 0); S-01:pk.calc.auxc (all-zero input gives zero area), pk.calc.tmax and pk.calc.tlast (NA when all zero).

### NCA-DAT-10 Missing dose
If the dose is missing, every parameter that needs it (CL, Vz, Vss, dose-normalised values) is NC with reason `dose_missing`; the others are computed.
- Status: `documented, untested`
- Sources: S-02:v05 (dose-dependent results are NA when no dose is given).

### NCA-DAT-11 Invalid dose or route
A dose that is not finite or is ≤ 0 is `InvalidDose`. An unknown route is `InvalidRoute`. An infusion without a duration > 0 is `InvalidInfusionDuration`.
- Status: `assumed`
- Sources: S-01:pk.nca.interval (duration is typically 0 for bolus and extravascular, non-zero for infusion); the errors themselves are design choices (golden rule 6).

## 4. Observed parameters

### NCA-OBS-01 Cmax
The largest concentration of the cleaned profile. NC when the profile is empty.
- Status: `documented, untested`
- Sources: S-01:pk.calc.cmax; S-08.

### NCA-OBS-02 Tmax
The time of the first occurrence of Cmax (`tmax_tie = first`, default) or of the last occurrence (`last`). NC when all concentrations are zero or the profile is empty.
- Status: `documented, untested`
- Sources: S-01:pk.calc.tmax; S-02:v40 (`first.tmax`, default true); S-08 (first maximum if not unique).

### NCA-OBS-03 Tlast and Clast
Tlast is the last time with C > 0; Clast is the observed concentration there. If every concentration is zero: Tlast NC, Clast 0.
- Status: `documented, untested`
- Sources: S-01:pk.calc.tlast, pk.calc.clast.obs; S-03 (0.11.0); S-02:v23.

### NCA-OBS-04 Tlag (extravascular only)
Tlag is the time of the sample immediately before the first sample whose concentration exceeds the concentration of the sample before it. If the profile rises from its very first step, Tlag is the first sample time (normally 0). NC when the profile never rises.
Sources describe it loosely ("time before the first concentration above LOQ or above the first concentration", "time associated with the first increasing concentration"; another tool: "time to observe the first non-zero concentration"). The published comparison table S-05 shows Tlag = 0 for a profile whose first non-zero value is at the second sample, which fits "the sample before", not "the first non-zero sample".
- Status: `assumed`
- Sources: S-01:pk.calc.tlag; S-05 (Table 3, read by H3); S-06.

### NCA-OBS-05 Dose normalisation
For the parameters Cmax, AUClast, AUCinf (observed and predicted), and optionally AUCall, AUMClast, AUMCinf and Clast: the dose-normalised value is the parameter divided by D. Result unit: the parameter's unit divided by the dose unit.
- Status: `documented, untested`
- Sources: S-01:pk.calc.dn; S-02:v40 (list of `.dn` parameters); S-05 (Table 1: Cmax and AUCinf per dose).

### NCA-OBS-06 Tfirst
Tfirst is the first time with C > 0. NC if every concentration is zero or missing. (T-003 found that for profiles that start at 0 it is later than the first sample.)
- Status: `documented, untested`
- Sources: S-01:pk.calc.tlast (its `pk.calc.tfirst` entry: time of the first concentration above the limit of quantification); T-003 expected values (`oracle/expected/*.csv`, parameter `tfirst`).

## 5. AUC and AUMC

### NCA-AUC-01 Domain
AUClast is the sum of the segment areas between consecutive points of the cleaned profile from t_d to Tlast. There is no area after Tlast.
- Status: `documented, untested`
- Sources: S-02:v05; S-02:v23.

### NCA-AUC-02 Linear segment
Between (t1, C1) and (t2, C2), with Δt = t2 − t1: area = Δt·(C1 + C2)/2.
- Status: `documented, untested`
- Sources: S-17 section 2.8.1 (via S-01:pk.calc.auxc); S-12; S-02:v23.

### NCA-AUC-03 Log segment
For C1 > 0, C2 > 0 and C1 ≠ C2: area = Δt·(C1 − C2)/ln(C1/C2). This is the exact integral of the exponential through both points (D: with k = ln(C1/C2)/Δt, the area is (C1 − C2)/k). It holds for rising and falling pairs alike.
- Status: `documented, untested`
- Sources: S-17 section 2.8.3 (via S-01:pk.calc.auxc); S-12; S-10; S-06 (the "Log" option).

### NCA-AUC-04 Zeros
A segment with C1 = C2 = 0 has area 0 under every method. A segment with exactly one zero end never uses the log formula. C1 = C2 > 0 uses the linear formula (the limit of the log formula).
- Status: `documented, untested`
- Sources: S-02:v23.

### NCA-AUC-05 Method `linear`
Every segment uses NCA-AUC-02 (with NCA-AUC-04 for zeros).
- Status: `documented, untested`
- Sources: S-02:v23; S-06 (`down = "Linear"`).

### NCA-AUC-06 Method `lin_up_log_down`
A segment uses the log formula when C1 > C2 > 0 (falling, both positive). Every other segment (rising, equal, or falling to zero) uses the linear formula. This is PKNCA's default.
- Status: `documented, untested`
- Sources: S-02:v23; S-01:pk.calc.auxc; S-06, S-07 (`down = "Log"` means linear-up, log-down); S-02:v05 (printed example, H1).

### NCA-AUC-07 Method `lin_log`
A segment that ends at or before Tmax uses the linear formula. A later segment uses the log formula when both ends are > 0 and C1 ≠ C2, whether it rises or falls; otherwise it is linear.
- Status: `documented, untested`
- Sources: S-02:v23; S-03 (0.11.0: added, "with required exceptions for zeros").

### NCA-AUC-08 Moment curve (AUMC)
The segment rule (which segments are linear and which are log) is the same as for the AUC of the chosen method. Per segment:
- linear: AUMC_seg = Δt·(t1·C1 + t2·C2)/2, a trapezoid on the product t·C;
- log (C1 > 0, C2 > 0, C1 ≠ C2), with k = ln(C1/C2)/Δt: AUMC_seg = (t1·C1 − t2·C2)/k + (C1 − C2)/k², equivalently (t1·C1 − t2·C2)·Δt/ln(C1/C2) + (C1 − C2)·Δt²/ln(C1/C2)²;
- C1 = C2 = 0: 0.

(D: integrate t·C1·exp(−k(t − t1)) from t1 to t2.) AUMClast sums segments up to Tlast. The abstract of S-10 reports that some commonly recommended linear forms of AUMC have a large mean error; the form used here is the one the compared tools use, kept for compatibility. Whether it is the form S-10 criticises was not checked.
- Status: `documented, untested`
- Sources: S-06, S-07 (linear and linear-up log-down AUMC); S-01:pk.calc.aumc; S-19 (via S-06); S-13; D.

### NCA-AUC-09 AUCall
AUCall = AUClast plus one extra segment: from (Tlast, Clast) to the first cleaned observation after Tlast, by the linear formula. Nothing after that point is integrated. If the last cleaned point is Tlast, AUCall = AUClast (for example when trailing BLQ points were dropped by the policy). AUMCall is analogous (a PKNCA parameter; the reference outputs listed in S-05 have no AUMCall).
- Status: `documented, untested`
- Sources: S-02:v05, S-02:v23; S-08 (AUCall includes the area down to the trailing zero).

### NCA-AUC-10 Test properties of the rules
(a) For data lying exactly on a mono-exponential, the log segments reproduce the analytic AUC and AUMC to rounding error; the linear rule overestimates each declining interval by the factor ((1 + e^−x)/2)·x/(1 − e^−x) with x = ln 2·Δt/t½ (1.0 %, 15.5 % and 57.1 % at Δt = 0.5, 2 and 4 half-lives; H4). (b) AUClast ≤ AUCall for non-negative data; and AUClast(lin-up/log-down) ≤ AUClast(linear) for non-negative data, because the logarithmic mean (C1 − C2)/ln(C1/C2) never exceeds the arithmetic mean (D).
- Status: `documented, untested`
- Sources: S-12 (the percentage figures); D; H4; worked example W2 and W7.

## 6. Terminal rate constant λz

### NCA-LZ-01 Regression
Over a point set P of n points (x = t, y = ln C), ordinary unweighted least squares:
- Sxx = Σ(x − x̄)², Sxy = Σ(x − x̄)(y − ȳ), Syy = Σ(y − ȳ)²;
- slope b = Sxy/Sxx, intercept a = ȳ − b·x̄, λz = −b;
- R² = Sxy²/(Sxx·Syy);
- adjusted R² = 1 − (1 − R²)(n − 1)/(n − 2), defined for n ≥ 3;
- Corr_XY = Sxy/√(Sxx·Syy), negative for a decaying profile (the published table in S-05 shows a negative value);
- Clast,pred = exp(a + b·Tlast).

If Syy = 0 (flat) the fit has λz = 0 and is invalid (NCA-LZ-04).
- Status: `documented, untested`
- Sources: S-01:pk.calc.half.life and `adj.r.squared`; S-02:v06 and v05 (printed example, H1); S-08 (formulas); S-06, S-07 (OLS); S-05.

### NCA-LZ-02 Eligible points (automatic selection)
From the cleaned profile, keep a point only if all of these hold:
1. C > 0 (zeros never enter the regression, whatever the BLQ policy);
2. its time is strictly after Tmax (unless `lambda_z_allow_tmax`); Tmax is always the one of the observed samples, never that of an inserted C0 (T-003 had to exclude the observed Cmax explicitly for exactly this reason);
3. its time is strictly after the end of the dose administration, t_d + T_inf (so no point during an infusion);
4. it is not flagged `lambda_z_exclude`.

- Status: `documented, untested`
- Sources: S-02:v06 (drop BLQ values, drop points at or before the end of the last dose including infusion duration, exclude Tmax by default, user exclusions); S-03 (0.12.1: only points after the end of the last administration); S-07 (selection restricted to samples after the end of an infusion).

### NCA-LZ-02b BLQ points that were given a positive value
A point that was BLQ (zero, or flagged) and was replaced by a positive number under `set(x)` is still BLQ in origin and never enters the λz regression. PKNCA works on values and names zeros as the points it drops, so its behaviour here is unknown.
- Status: `assumed`
- Sources: reader's choice, for consistency with NCA-DAT-06; test O-07.

### NCA-LZ-03 Candidate sets
Let the eligible points be e_1 < … < e_m in time (e_m is Tlast). The candidates are the sets {e_{m−n+1}, …, e_m} for n = `lambda_z_min_points` (default 3) up to m. If m < min points, λz is NC with reason `too_few_points`.
Example (reader's own): samples at 0, 2, 4, 6, 8, 12, 24 h with positive concentrations from 2 h on, Tmax = 2 h, Tlast = 24 h. Eligible points: 4, 6, 8, 12, 24. Candidates: {8, 12, 24}, {6, 8, 12, 24}, {4, 6, 8, 12, 24}. With Tmax allowed, {2, 4, 6, 8, 12, 24} is added; with a minimum of 4 points, {8, 12, 24} disappears.
- Status: `documented, untested`
- Sources: S-02:v06; S-01:pk.calc.half.life; S-06, S-07 (sequential fits from the last point backwards, at least 3 points).

### NCA-LZ-04 Valid fit
A candidate fit is valid only if λz > 0.
- Status: `documented, untested`
- Sources: S-01:pk.calc.half.life; S-02:v06; S-09 (λz fails if the slope is positive).

### NCA-LZ-05 Selection, tolerance reading (primary)
Let f = `lambda_z_tolerance` (1e-4) and M the largest adjusted R² among the fits that compete (NCA-LZ-07). Admissible fits are the valid ones with adjusted R² ≥ M − f. Choose the admissible fit with the most points. If there is none, λz is NC with reason `no_valid_fit`.
The descriptions of the reference software's own rule in S-06 and S-07 agree with this one (a larger adjusted R² wins; a difference below the tolerance counts as zero; then the longer set wins).
- Status: `documented, untested`
- Sources: S-01:pk.calc.half.life (rules in order: enough points; λz > 0 with the best adjusted R² within the factor; most points); S-02:v06; S-07 and S-06 (BestSlope); S-04 (poster; the reference software chooses the final set by best adjusted R²).

### NCA-LZ-06 Selection, "bonus" reading (to be decided by the oracle)
The PKNCA options text can also be read as: score = adjusted R² + f·n, choose the valid fit with the largest score (ties to more points). The two readings agree whenever the best fit is clearly separated, and differ on profile D1 of worked example W6. The oracle agent must run PKNCA on D1, record which point set it returns, and write it in the oracle options file. Implement both as `lambda_z_tie_rule = tolerance | bonus`.
- Status: `documented, untested`
- Sources: S-02:v40 (`adj.r.squared.factor`: "this factor times the number of data points added"); S-01:pk.calc.half.life (argument text: allowance for adding another point); S-01 and S-02:v06 (tolerance wording); H5 (D1).

### NCA-LZ-07 Order of the positive-slope filter
- PKNCA: M is the largest adjusted R² among all candidate fits, including those with λz ≤ 0; fits with λz ≤ 0 are discarded afterwards. If the best-scoring fit has λz ≤ 0, λz can be NC even when a positive-λz fit exists.
- Reference software (according to S-04): only descending-slope (λz > 0) candidates compete.

Option `lambda_z_positive_filter_first`: false in the PKNCA profile, true in the reference profile. Worked example W6 profile D2 separates them.
- Status: `documented, untested`
- Sources: S-02:v06 ("rules must be met simultaneously … the half-life may end up being unreportable"); S-04 (poster conclusions, explaining small half-life differences between the programs).

### NCA-LZ-08 Manual selection
When the user supplies the points (per-point flags or an inclusive time range): no automatic selection happens; `lambda_z_min_points`, tolerance and `lambda_z_allow_tmax` are ignored; at least 2 points are needed; zero (BLQ) points cannot be used; λz ≤ 0 gives NC; for n = 2 adjusted R² is NC with a warning. Manual selection may include the Tmax point.
- Status: `documented, untested`
- Sources: S-01:pk.calc.half.life (manually selected points); S-02:v06; S-03 (0.9.2: two-point half-life works, adjusted R² warns; 0.12.0: negative half-life no longer allowed manually).

### NCA-LZ-09 Excluding points but keeping automatic selection
Points flagged `lambda_z_exclude` are removed from the eligible set before the candidate sets are built.
- Status: `documented, untested`
- Sources: S-02:v06 (printed result for one excluded point).

### NCA-LZ-10 Outputs of the fit
half-life = ln 2/λz; `lambda_z_t_first` and `lambda_z_t_last` = times of the first and last point used; `lambda_z_n_points` = n; span ratio = (t_last_used − t_first_used)/half-life (equivalently (t_last_used − t_first_used)·λz/ln 2); Clast,pred as in NCA-LZ-01.
- Status: `documented, untested`
- Sources: S-02:v05 (printed example, H1); S-01:pk.calc.half.life (value list); S-08 (span formula).

### NCA-LZ-11 Failure propagation
If λz is NC, then half-life, Clast,pred, span ratio, AUCinf, AUMCinf, MRT to infinity, CL, Vz and Vss are NC with the reason of the failure; Cmax, Tmax, Tlast, Clast, AUClast, AUCall, AUMClast and MRTlast are still computed.
- Status: `documented, untested`
- Sources: S-03 (0.12.1: the predicted-Clast interval AUC is NA when the half-life is not estimable; 0.9.0: reason recorded when too few points); S-02:v05.

### NCA-LZ-12 PKNCA's quality thresholds do not suppress results
PKNCA's default thresholds for its optional exclusion helpers are 0.9 (R² and adjusted R²), 2 (span ratio) and 20 % (extrapolated AUC). In 0.12.1 they are not applied during the calculation: the printed vignette example reports a λz whose R² is 0.76. Another tool uses a default minimum adjusted R² of 0.7. The reference software's own thresholds are unknown.
- Status: `documented, untested`
- Sources: S-02:v40; S-01:exclude_nca; S-02:v05; S-06.

### NCA-LZ-12b Quality flags
λz is always reported when estimable. Quality problems are flagged, not hidden: adjusted R² below a threshold, span ratio below a threshold, extrapolated AUC above a threshold, fewer points than a threshold. The thresholds are inputs; PKNCA's values are the starting point; the flags travel with the result to the UI, the CLI and the MCP server.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 (a poor fit must be flagged); design choice.

### NCA-LZ-13 Back-extrapolated C0 is never a regression point
A concentration produced by back-extrapolation (NCA-IV-01) is not an observation and is never used in the λz regression.
- Status: `documented, untested`
- Sources: S-09 (stated as an exclusion from the regression); follows from NCA-LZ-02.

### NCA-LZ-14 Tmax point for IV bolus data
PKNCA leaves the Tmax point out of the regression whatever the route. Another tool leaves the maximum point out for non-bolus data only, which keeps it eligible for IV bolus. Whether the reference software does the same is unknown. Reference profile: `lambda_z_allow_tmax = true` for `iv_bolus` (hypothesis, to be tested on IV bolus private data); PKNCA profile: false.
- Status: `assumed`
- Sources: S-02:v40 (`allow.tmax.in.half.life` false); S-09.

## 7. Extrapolation and derived parameters

### NCA-EXT-01 AUC to infinity
AUCinf,obs = AUClast + Clast/λz. AUCinf,pred = AUClast + Clast,pred/λz. The part before Tlast is the same in both; the observed curve is not modified.
- Status: `documented, untested`
- Sources: S-02:v05 and v23 (printed example, H1); S-08; S-01:pk.calc.auxc.

### NCA-EXT-02 Percent extrapolated
AUC %extrap = 100·(1 − AUClast/AUCinf), for the observed and the predicted version. NC if AUCinf is not finite or ≤ 0, or AUClast ≤ 0.
- Status: `documented, untested`
- Sources: S-01:pk.calc.aucpext; S-08.

### NCA-EXT-03 AUMC to infinity
AUMCinf = AUMClast + Clast·Tlast/λz + Clast/λz² (observed), with Clast,pred in place of Clast for the predicted version. AUMC %extrap = 100·(1 − AUMClast/AUMCinf). (D: the integral of t·Clast·e^(−λz(t − Tlast)) from Tlast to infinity.)
- Status: `documented, untested`
- Sources: S-08 (printed formula); D; H3 (S-05 values reproduced).

### NCA-EXT-04 Mean residence time
MRT = AUMC/AUC for extravascular data (it includes the absorption time). For IV data, MRT = AUMC/AUC − T_inf/2 (T_inf = 0 for a bolus). Variants: to Tlast (AUMClast and AUClast), to infinity observed, to infinity predicted.
- Status: `documented, untested`
- Sources: S-01:pk.calc.mrt (formula with the infusion half-duration); S-08 (formulas by route); S-13; S-05 (Table 3 MRT values, H3).

### NCA-EXT-05 Clearance
CL = D/AUCinf (observed or predicted version). For extravascular data the number is the apparent clearance CL/F; only the label changes.
- Status: `documented, untested`
- Sources: S-01:pk.calc.cl; S-17 section 2.5.1 (via S-01); S-08; S-05 (H3).

### NCA-EXT-06 Volume of the terminal phase
Vz = D/(λz·AUCinf) = CL/λz (observed or predicted). Extravascular: the apparent Vz/F.
- Status: `documented, untested`
- Sources: S-01:pk.calc.vz; S-08; S-05 (H3); S-03 (0.7.1 corrected a Vz bug).

### NCA-EXT-07 Volume at steady state (IV only)
Vss = CL·MRT_iv, with the IV form of MRT of NCA-EXT-04, i.e. D·AUMC/AUC² − D·T_inf/(2·AUC). Observed and predicted versions. Not reported for extravascular data. PKNCA has a plain and an IV-corrected Vss; for a bolus they coincide, and for an infusion the corrected one is required.
- Status: `documented, untested`
- Sources: S-01:pk.calc.vss; S-14; S-15; S-08.

### NCA-EXT-08 Division by zero
Any ratio with a zero or non-finite denominator (for example CL when AUC = 0) is NC, never an infinity.
- Status: `documented, untested`
- Sources: S-03 (0.8.2).

## 8. Intravenous bolus

### NCA-IV-01 C0 estimation
Applies to `iv_bolus`. Methods are tried in order and the first that gives a value is used:
1. the observed concentration at t_d, if it is > 0;
2. log-linear back-extrapolation through the first two cleaned post-dose points (t1, C1), (t2, C2): k = ln(C1/C2)/(t2 − t1), C0 = C1·exp(k·(t1 − t_d)); not valid unless 0 < C2 < C1;
3. the first post-dose concentration C1.

For `extravascular` and `iv_infusion` single-dose data C0 is 0. C0 is reported for IV bolus only. Cmax and Tmax are never replaced by C0.
- Status: `documented, untested`
- Sources: S-01:pk.calc.c0 (methods and their usual order by route); S-08 (log-linear regression of the first two points; zero otherwise); S-09 (fallback to the first measurement if the pair is unusable); D.

### NCA-IV-02 AUC with a back-extrapolated start (PKNCA construction)
PKNCA computes separate "IV" AUC parameters: they need a record at t_d (any concentration); they add to the ordinary AUC the difference between the first segment drawn from (t_d, C0) and the first segment drawn from the record actually at t_d. The result equals integrating the profile with the t_d concentration replaced by C0, the first segment following the chosen AUC method (log when C0 > C1 under lin-up/log-down). Variants: last, all, infinity.
T-003 chose the equivalent construction for the public oracle: C0 (log-linear through the first two positive points) is added as a profile point at t_d, Cmax, Tmax and Tfirst come from the observed samples only, and the added point is kept out of λz. The reader's recomputation (hand check H6) shows that this reproduces the expected AUC, AUMC and derived values of the indomethacin cases, so for the public oracle `start_policy = c0` is the way to run IV bolus data that have no sample at t_d.
- Status: `documented, untested`
- Sources: S-01:pk.calc.auciv; S-02:v40 (parameter list: `aucivlast`, `aucivall`, `aucivinf.obs` …).

### NCA-IV-03 Percent back-extrapolated, PKNCA form
100·(1 − AUC/AUC_iv), with AUC_iv from NCA-IV-02 and AUC the matching ordinary one. Requires the record at t_d.
- Status: `documented, untested`
- Sources: S-01:pk.calc.auciv (`pk.calc.auciv_pbext`).

### NCA-IV-04 Reference-profile reading of bolus AUC (hypothesis)
In the reference profile, for IV bolus data AUClast, AUCall and AUCinf already include the segment from (t_d, C0) to the first sample (that is, `start_policy = c0`), and "% back-extrapolated" is the share of AUCinf that comes from that first segment: 100·A_bk/AUCinf, where A_bk is the area from (t_d, C0) to (t1, C1). The two forms differ when a placeholder record exists at t_d. Worked example W4 gives numbers for the second form.
- Status: `assumed`
- Sources: S-08 (percent of AUCinf due to back-extrapolation; C0 used by all parameters except Cmax, Tmax); S-09.

## 9. Edge cases (golden rule 6)

Every row needs a regression test that cites its rule. "NC" results carry the reason shown.

| condition | behaviour | rule |
|---|---|---|
| NaN or empty concentration | treated as missing, removed | DAT-03 |
| infinite concentration | error `InfiniteConcentration` | DAT-02 |
| zero concentration | BLQ policy | DAT-05, DAT-06 |
| negative concentration | error `NegativeConcentration` (default policy) | DAT-04 |
| value below the LOQ | per policy | DAT-06 |
| duplicated times | error `DuplicateTime` naming the time | DAT-02 |
| unsorted times | error `UnsortedTime` naming the index | DAT-02 |
| NaN or infinite time | error `NonFiniteTime` | DAT-02 |
| arrays of different length, or empty | errors `LengthMismatch`, `EmptyProfile` | DAT-02 |
| one point left | AUC and AUMC NC (`single_point`) | DAT-09 |
| all concentrations zero | AUC = 0, Cmax = 0, Clast = 0; Tmax, Tlast, λz NC | DAT-09 |
| first sample after t_d, `start_policy = none` | AUC NC (`no_start_concentration`) | DAT-08 |
| fewer than 3 eligible points | λz NC (`too_few_points`) and dependants NC | LZ-03, LZ-11 |
| best fit has λz ≤ 0, or no valid fit | λz NC (`no_valid_fit`) | LZ-04, LZ-05, LZ-07 |
| Tmax is the last sample | no eligible point, λz NC | LZ-02, LZ-03 |
| flat selected points (Syy = 0) | fit invalid | LZ-01, LZ-04 |
| manual selection with 2 points | allowed; adjusted R² NC with warning | LZ-08 |
| AUCinf ≤ 0 or AUClast ≤ 0 | percent extrapolated NC | EXT-02 |
| any division by zero | NC, never infinity | EXT-08 |
| dose missing | dose-dependent parameters NC (`dose_missing`) | DAT-10 |
| dose ≤ 0, bad route, bad infusion duration | errors | DAT-11 |
| IV bolus with unusable first pair | C0 falls back to the next method | IV-01 |
| singular matrix, non-convergence | not applicable: the regression has a closed form and distinct times make Sxx > 0 | LZ-01 |

## 10. Units, dose normalisation, vocabulary

### NCA-UNIT-01 Derived units
With time unit [T], concentration unit [C] and dose unit [D]:

| quantity | unit |
|---|---|
| Cmax, Clast, Clast,pred, C0 | [C] |
| Tmax, Tlast, Tlag, half-life, λz window times, MRT | [T] |
| λz | 1/[T] |
| AUC (all variants) | [T]·[C] |
| AUMC (all variants) | [T]²·[C] |
| CL, CL/F | [D]/([T]·[C]) |
| Vz, Vz/F, Vss | [D]/[C] |
| R², adjusted R², Corr_XY, span ratio, number of points, percent values | dimensionless |
| dose-normalised value | parameter unit / [D] |

The dose unit may be a plain amount or an amount per body weight; units propagate formally (for example (mg/kg)/(h·mg/L) reduces to L/h/kg).
- Status: `documented, untested`
- Sources: S-02:v07 (derived unit strings in its examples); S-05 (Table 3 units).

### NCA-UNIT-02 Unit compatibility check
Before running, check the units together: CL and V reduce to volume (per time) only if the dose and concentration share a mass or amount dimension. If the dose is a mass and the concentration is molar (or the reverse), warn that CL and V need a molecular weight (optional input) and offer to convert. Infusion duration and λz-related times must use the time unit of the profile; a mismatch is an error. Conversions happen only on request, through explicit factors; otherwise results stay in the units implied by the inputs.
- Status: `assumed`
- Sources: S-02:v07 (conversion between mass and molar units needs extra information); S-06 (molecular weight argument); `AGENTS.md` section 7, friction 5.

### Vocabulary table

One row per in-scope parameter. Inside `caladrius_nca` results are looked up by PKNCA name (the test-side API fixed by T-003, `NcaResult::get(name)`). "Proposed id" is the user-facing name for the CLI, the MCP server, the UI and exports; the mapping between the two columns is one to one, and neither may be renamed without updating this table. PKNCA names are from S-01 and S-02. Reference-software names are the output labels as printed in S-05 (its Table 1) where it lists them; a label marked ᵃ is not in S-05 and comes only from S-08 (another tool that uses the same labels), so it is `assumed` for the reference software. Route-dependent labels are shown as IV / extravascular. CDISC codes (S-06, S-08) are given for orientation.

| proposed id | what it is | PKNCA | reference software | CDISC | name status |
|---|---|---|---|---|---|
| `cmax` | maximum observed concentration | cmax | Cmax | CMAX | documented, untested |
| `cmax_dn` | Cmax per dose | cmax.dn | Cmax_D | CMAXD | documented, untested |
| `tmax` | time of Cmax | tmax | Tmax | TMAX | documented, untested |
| `tlag` | lag before first rise (extravascular) | tlag | Tlag | TLAG | documented, untested |
| `clast_obs` | last concentration above zero | clast.obs | Clast | CLST | documented, untested |
| `clast_pred` | Clast from the λz line | clast.pred | Clast_pred | CLSTP | documented, untested |
| `tlast` | time of Clast | tlast | Tlast | TLST | documented, untested |
| `tfirst` | time of first positive concentration | tfirst | (no label found in the sources) | (none) | documented, untested (PKNCA only) |
| `lambda_z` | terminal rate constant | lambda.z | Lambda_z | LAMZ | documented, untested |
| `half_life` | ln 2/λz | half.life | HL_Lambda_z | LAMZHL | documented, untested |
| `lambda_z_t_first` | first time used for λz | lambda.z.time.first | Lambda_z_lower | LAMZLL | documented, untested |
| `lambda_z_t_last` | last time used for λz | lambda.z.time.last | Lambda_z_upper | LAMZUL | documented, untested |
| `lambda_z_n_points` | points used for λz | lambda.z.n.points | No_points_Lambda_z | LAMZNPT | documented, untested |
| `r_squared` | R² of the λz fit | r.squared | Rsq | R2 | documented, untested |
| `adj_r_squared` | adjusted R² | adj.r.squared | Rsq_adjusted | R2ADJ | documented, untested |
| `corr_xy` | correlation of time and ln C | (not output) | Corr_XY | CORRXY | documented, untested |
| `span_ratio` | window length over half-life | span.ratio | Span ᵃ | (none) | assumed (reference label) |
| `auc_last` | AUC to Tlast | auclast | AUClast | AUCLST | documented, untested |
| `auc_all` | AUC to last observation | aucall | AUCall | AUCALL | documented, untested |
| `auc_inf_obs` | AUC to infinity, observed Clast | aucinf.obs | AUCINF_obs | AUCIFO | documented, untested |
| `auc_inf_pred` | AUC to infinity, predicted Clast | aucinf.pred | AUCINF_pred | AUCIFP | documented, untested |
| `auc_inf_obs_dn` | per dose | aucinf.obs.dn | AUCINF_D_obs | AUCIFOD | documented, untested |
| `auc_inf_pred_dn` | per dose | aucinf.pred.dn | AUCINF_D_pred | AUCIFPD | documented, untested |
| `auc_pct_extrap_obs` | percent extrapolated, observed | aucpext.obs | AUC_%Extrap_obs (spelling varies across sources) | AUCPEO | documented, untested |
| `auc_pct_extrap_pred` | percent extrapolated, predicted | aucpext.pred | AUC_%Extrap_pred | AUCPEP | documented, untested |
| `auc_pct_back_extrap_obs` | percent back-extrapolated (IV bolus) | aucivpbextinf.obs (different formula, NCA-IV-03) | AUC_%Back_Ext_obs ᵃ | AUCPBEO | assumed (reference label and formula) |
| `auc_pct_back_extrap_pred` | same, predicted | aucivpbextinf.pred | AUC_%Back_Ext_pred ᵃ | AUCPBEP | assumed |
| `aumc_last` | AUMC to Tlast | aumclast | AUMClast | AUMCLST | documented, untested |
| `aumc_inf_obs` | AUMC to infinity, observed | aumcinf.obs | AUMCINF_obs | AUMCIFO | documented, untested |
| `aumc_inf_pred` | AUMC to infinity, predicted | aumcinf.pred | AUMCINF_pred | AUMCIFP | documented, untested |
| `aumc_pct_extrap_obs` | percent extrapolated AUMC | (not output) | AUMC_%Extrap_obs | AUMCPEO | documented, untested |
| `aumc_pct_extrap_pred` | same, predicted | (not output) | AUMC_%Extrap_pred | AUMCPEP | documented, untested |
| `mrt_last` | MRT to Tlast | mrt.last, mrt.iv.last | MRTlast | MRTEVLST, MRTIVLST | documented, untested |
| `mrt_inf_obs` | MRT to infinity, observed | mrt.obs, mrt.iv.obs | MRTINF_obs | MRTEVIFO, MRTIVIFO | documented, untested |
| `mrt_inf_pred` | MRT to infinity, predicted | mrt.pred, mrt.iv.pred | MRTINF_pred | MRTEVIFP, MRTIVIFP | documented, untested |
| `cl_obs` | clearance from AUCinf,obs | cl.obs | Cl_obs ᵃ / Cl_F_obs | CLO / CLFO | assumed (IV label) |
| `cl_pred` | clearance from AUCinf,pred | cl.pred | Cl_pred ᵃ / Cl_F_pred | CLP / CLFP | assumed (IV label) |
| `vz_obs` | terminal volume, observed | vz.obs | Vz_obs ᵃ / Vz_F_obs | VZO / VZFO | assumed (IV label) |
| `vz_pred` | terminal volume, predicted | vz.pred | Vz_pred ᵃ / Vz_F_pred | VZP / VZFP | assumed (IV label) |
| `vss_obs` | volume at steady state, observed (IV) | vss.obs, vss.iv.obs | Vss_obs ᵃ | VSSO | assumed (reference label) |
| `vss_pred` | same, predicted (IV) | vss.pred, vss.iv.pred | Vss_pred ᵃ | VSSP | assumed (reference label) |
| `c0` | back-extrapolated start (IV bolus) | c0 | C0 ᵃ | C0 | assumed (reference label) |

Notes: the reference-name column must be checked against the column headers of the human's exports before any test relies on it (names only; no number from `private/` enters a versioned file). S-05's Table 1 prints some labels with small typos; the spellings above are the ones that are consistent across sources.

## 11. Worked examples

The numbers follow from the rules above. They are candidate unit tests and "independent computation" cases for `caladrius-testkit`. W1 and W8 use numbers printed in PKNCA's documentation (row in `ATTRIBUTION.md`); the others are the reader's own arithmetic (H, D), computed with a script kept outside the repository. Their status is that of the rules they exercise (`documented, untested`).

**Cross-check H6 against the public oracle.** The reader applied sections 3 to 8 (PKNCA profile, selection by the tolerance reading, positive-slope filter after selection) to all 18 profiles of T-003 (12 theophylline, 6 indomethacin, both `lin up/log down` and `linear`) in a throwaway script. Every parameter it computes (Cmax, Tmax, Tlast, Clast, λz, R², adjusted R², Clast,pred, half-life, span ratio, AUClast, AUMClast, AUCinf and AUMCinf observed and predicted, % extrapolated, CL, Vz, MRT, Vss, C0) agrees with `oracle/expected/*.csv` to better than 1e-14 relative, and the selected λz window (first time, number of points) is identical. The three tie readings of NCA-LZ-05/06/07 coincide on all 18 profiles, so these data do not settle O-01 or O-02. The status of the rules stays `documented, untested` until a versioned engine test passes.

**W1. Nine-point profile (PKNCA profile, no dose).** t = 0, 1, 2, 3, 4, 5, 8, 12, 24; C = 0, 2.5, 3, 2, 1.5, 1.2, 1.1, 0, 0. Defaults: lin-up/log-down; BLQ first keep, middle drop, last keep.
Tmax 2, Tlast 8, Clast 1.1. Eligible λz points (after Tmax, C > 0): 3, 4, 5, 8. Candidates: n = 3 (4, 5, 8): adjusted R² 0.4902; n = 4 (3, 4, 5, 8): adjusted R² 0.6370, selected. λz = 0.1075592, R² = 0.7580245, adjusted R² = 0.6370368, Clast,pred = 1.0216136, half-life = 6.4443313, span ratio = 0.7758757.
AUClast = 12.9965842; AUCall = 15.1965842 (adds the linear triangle 8 → 12 of area 0.5·4·1.1 = 2.2); AUCinf,obs = 23.2235095; AUCinf,pred = 22.4947355. Source: values printed in S-02:v05; reproduced by H1.

**W2. Exact mono-exponential, IV bolus.** C = 10·e^(−0.2 t) at t = 0, 1, 2, 4, 8, 12; D = 100. Observed value at t = 0, so C0 = 10.
Lin-up/log-down: AUClast = 45.46410233553 and AUMClast = 172.88973970400, both equal to the analytic integrals over [0, 12]. `linear`: AUClast = 46.61219693041, AUMClast = 168.59557915366. Eligible λz points: 1, 2, 4, 8, 12 (Tmax = 0 excluded); every fit is exact, so the tolerance reading selects all 5 points; λz = 0.2 to rounding error. Then AUCinf,obs = 50, AUMCinf,obs = 250, MRT = 5, CL = 2, Vz = 10, Vss = 10, half-life = 3.4657359028 (all to ~1e-12 relative). Vss = D/C0 = 10 as theory requires.

**W3. BLQ in the middle and at the end.** t = 0, 1, 2, 3, 4, 6, 8, 12; C = 0, 4, 8, 0, 3, 2, 1, 0. Tmax = 2, Tlast = 8. Lin-up/log-down.
- middle `drop` (point at 3 removed; first and last `keep`): AUClast = 26.01345148476, AUMClast = 85.14346931138, AUCall = 28.01345148476.
- middle `keep`: AUClast = 21.31799700653, AUMClast = 70.19764732525, AUCall = 23.31799700653.
In both cases AUCall − AUClast = 2 (the triangle 8 → 12 with Clast = 1).

**W4. IV bolus without a sample at t_d.** D = 50, t = 0.25, 0.5, 1, 2, 4; C = 8.0, 6.4, 4.1, 1.7, 0.29. Tmax = 0.25, Cmax = 8.
C0 from the first two points: k = ln(8/6.4)/0.25, C0 = 8·1.25 = 10 exactly. With `start_policy = c0` (lin-up/log-down): A_bk (area from t_d to 0.25) = 0.5/ln 1.25 = 2.24071005886; AUClast = 10.93647360532; AUMClast = 11.05095097097; AUC from the first sample only = 8.69576354646.
λz, PKNCA profile (Tmax point excluded; eligible 0.5, 1, 2, 4): n = 3 {1, 2, 4}: adjusted R² 0.99999762; n = 4 {0.5, 1, 2, 4}: 0.99999794, selected (both inside the tolerance, more points win). λz = 0.88364213063, R² = 0.99999862877, Clast,pred = 0.29004192056, half-life = 0.78442070216.
Then AUCinf,obs = 11.26466076385, AUMCinf,obs = 12.73510240167, MRT = 1.13053581183, AUC %extrap = 2.91342247585, CL = 4.43866007581, Vz = 5.02314219973, Vss = 5.01806417225.
Reference-reading percent back-extrapolated (NCA-IV-04) = 100·A_bk/AUCinf,obs = 19.89150055946. The PKNCA form (NCA-IV-03) needs a record at t_d and is not defined for this profile as given.

**W5. Infusion identity.** D = 100, T_inf = 2, AUCinf = 50, AUMCinf = 300, λz = 0.2. MRT_iv = 300/50 − 2/2 = 5; CL = 2; Vz = 10; Vss = 2·5 = 10. (Theory: a one-compartment disposition with k = 0.2 and V = 10 gives AUMC/AUC = 1/k + T_inf/2 = 6, and Vss = V.)

**W6. Discriminating λz profiles (for the oracle, section 12 items O-01 and O-02).**
D1: t = 0, 0.5, 1, 2, 4, 6, 8, 12, 24; C = 0, 5.0, 9.0, 8.817, 6.294, 4.557, 3.466, 1.85, 0.274. Tmax = 1; eligible points 2, 4, 6, 8, 12, 24.

| n | first time used | λz | adjusted R² |
|---|---|---|---|
| 3 | 8 | 0.15872853 | 0.99998469 |
| 4 | 6 | 0.15716733 | 0.99962744 |
| 5 | 4 | 0.15672669 | 0.99970756 |
| 6 | 2 | 0.15703919 | 0.99976306 |

Tolerance reading (f = 1e-4): M = 0.99998469; the others are 1.2e-4 to 2.2e-4 below it, so only n = 3 is admissible: λz = 0.15872853. Bonus reading: scores 1.00028469 (n = 3), 1.00022744, 1.00020756, 1.00036306 (n = 6): n = 6 wins, λz = 0.15703919. The oracle agent runs PKNCA on D1: λz 0.1587 means tolerance, 0.1570 means bonus.
D2: t = 0, 0.5, 1, 2, 4, 6, 8, 12, 24; C = 0, 5.0, 9.0, 6.0, 4.0, 3.0, 2.0, 2.5, 3.0. Candidates: n = 3 (8, 12, 24): λz = −0.02299970, adjusted R² 0.7787 (rising); n = 4: −0.00982438, −0.2463; n = 5: 0.00411282, −0.3116; n = 6: 0.02068158, −0.0192. PKNCA reading (best score first, then discard non-positive λz): λz NC. Positive-filter-first reading: only n = 5 and n = 6 compete; n = 6 (adjusted R² −0.0192) is the best, λz = 0.02068158. The result is of no practical value, which is the point: a quality flag must catch it (NCA-LZ-12).

**W7. Linear-rule bias on an exponential (test property).** One interval of a pure exponential with Δt = n half-lives: the linear rule overestimates the area by the factor ((1 + e^−x)/2)·x/(1 − e^−x), x = n·ln 2: 1.00999 (n = 0.5), 1.15525 (n = 2), 1.57113 (n = 4). The log rule gives factor 1 exactly. Source: S-12 (the 1 %, 15.5 %, 57.1 % figures); D; H4.

**W8. Public theophylline data, subject 1 (targets for T-003, PKNCA profile, no dose).** As printed in S-02:v06: default selection gives 3 points with the first time at 9.05 h and λz = 0.0485, adjusted R² ≈ 1, Clast,pred = 3.28, half-life = 14.3 h, span ratio = 1.07. Excluding the point at 12.12 h gives 4 points, first time 5.1 h, λz = 0.0482, adjusted R² = 0.999, half-life = 14.4 h. Manual selection of all points after 3 h gives 6 points, first time 3.82 h, λz = 0.0475, R² = 0.999, adjusted R² = 0.998, Clast,pred = 3.30. The reader reproduced these with unrounded values 0.048457, 0.048183 and 0.047514 (H2). The data themselves are exported by T-003.

## 12. Open items, assumptions to test, and what is not specified

Each open item names the card or person who can settle it.

| id | item | rule | who and how |
|---|---|---|---|
| O-01 | Which tie reading does PKNCA implement (`tolerance` or `bonus`)? The 18 public profiles do not discriminate (H6) | LZ-05, LZ-06 | oracle agent: run PKNCA on W6/D1 (one R call), record the result in an options file or a new case; the engine implements both |
| O-02 | Positive-slope filter order, PKNCA versus reference | LZ-07 | T-003 on D2 for PKNCA; the private oracle for the reference |
| O-03 | Exact definition of Tlag | OBS-04 | T-003: pick a public profile with a lag and compare with PKNCA; private oracle for the reference |
| O-04 | Reference-software IV bolus AUC convention and definition of percent back-extrapolated (the PKNCA side is settled for the oracle by T-003's added C0 point, see IV-02) | IV-03, IV-04 | private oracle (IV bolus subjects); the `aucivpbext*` parameters are not in the public oracle |
| O-05 | Is the Tmax point eligible for λz with IV bolus data in the reference software? | LZ-14 | private oracle (IV bolus subjects) |
| O-06 | Default AUC method and BLQ rule of the reference software | section 2.2 | question Q-008 (the human reads the settings on his own screen) |
| O-07 | BLQ points replaced by a positive number: excluded from λz in PKNCA? | LZ-02 | T-003: a profile with `set(x)` for the last BLQ points |
| O-08 | Default `start_policy` | DAT-08 | orchestrator, after Q-008 |
| O-09 | Negative-concentration default | DAT-04 | orchestrator |
| O-10 | PKNCA applies log whenever the later value is non-zero; the guard "> 0" differs only for negative values | DAT-04, AUC-06 | T-003 only if `allow` is implemented |
| O-11 | PKNCA version drift: this file follows 0.12.1 (CRAN 2025-08-19) | all PKNCA rules | T-003 records versions; a newer CRAN version means re-reading the NEWS file |
| O-12 | Textbook section and page numbers | AUC, LZ, EXT | someone holding the books; S-17 and S-18 are cited at book level or through S-01 only |
| O-13 | Spelling of reference output labels | section 10 | compare with the headers of the human's exports |
| O-14 | Hand check H3 compares published reference numbers; wording and use need the human's decision | sources.md H3 | question Q-007 |
| O-15 | Engine-side lookup is by PKNCA name (T-003 API); user-facing ids differ (section 10) | section 10 | interface agent maps the two columns; no engine change |

**Not specified here** (candidates for later cards, none started): partial or interval AUC with interpolation and extrapolation (PKNCA extrapolates beyond Tlast with the log rule whatever the method); steady-state and multiple-dose parameters (AUCτ, Cavg, fluctuation, accumulation, λz after the last dose); urine and excretion parameters; sparse sampling; effective half-life and Kel; AUC above or time above a threshold; bioavailability and ratios; weighted λz regression; superposition.
