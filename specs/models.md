# One-compartment models: behaviour specification

Card T-008, synced with the model oracle and the engine by card T-017. Written by the `reader` agent on 2026-10-08, in its own words and formulas. Sources are cited by the keys of `specs/sources.md` (S-01 …; the keys added for this file are in its section 7); `D` is a derivation shown here, `H` a hand or scripted check by the reader (section 9).

## 0. How to read this file

**Status tags** (see `specs/README.md`): `confirmed by oracle`, `documented, untested`, `assumed`. The model oracle (card T-009) now exists: 21 cases and 886 expected values, produced by an independent implementation in 256-bit arithmetic rounded once to double, cross-checked within 1e-8 against a matrix exponential and an adaptive ODE solution that also integrates the AUC and the first moment; the tolerance is 1e-12 relative, an expected zero must be exact. 73 oracle tests pass against the engine (card T-010). A rule is `confirmed by oracle` when those cases exercise it; the cases are named in the line `Oracle:` of the rule. A choice made for Caladrius (names, validation, numerical safeguards the oracle does not exercise) is `assumed`.

**Rule ids.** `MOD-<AREA>-<nn>`. Areas: GEN (conventions and domains), IVB (intravenous bolus), IVI (intravenous infusion), AB1 (first-order absorption), AB0 (zero-order absorption), SEC (secondary parameters), NUM (numerics), MD (multiple dosing, later), VOC (vocabulary), 2C (two compartments, section 11).

**Clean room.** The reference software's own documentation was not used (`specs/sources.md` section 2). The textbooks cannot be read by the reader (S-17, S-18 are cited at book level only). The equations below are therefore derived (D) from the linear differential equations of the compartment diagram, which is how the textbooks obtain them, and compared with the model catalogue and the CL/V convention of two open packages (S-20, S-21). Where a rule is a design choice it says so.

**Scope (AGENTS.md section 1).** One compartment, linear first-order elimination, single dose, closed form. Two compartments are specified in section 11 (card T-031); user-written ODE models and several subjects are later steps. Multiple dosing is noted in MOD-MD-01 and not specified further.

**Numerics.** Double precision. Nothing is rounded inside the computation. A function value that is not finite is an error (MOD-NUM-02), never a silent NaN. The engine evaluates numerically stable rearrangements of the formulas below, which are the same functions (section 8, MOD-AB1-03, MOD-NUM-01); a form that differs from the textbook one only by rounding is not a method difference and has no entry in `specs/differences.md`.

## 1. Notation

| symbol | meaning | unit |
|---|---|---|
| t | time since the dose (the dose time t_d is subtracted by the caller; for t < 0 the concentration is 0, MOD-GEN-02) | [T] |
| D | dose amount actually reaching the system (for extravascular models D is the administered dose times F, MOD-GEN-03) | [D] |
| V | volume of distribution of the central compartment (apparent V/F for extravascular input) | [D]/[C] |
| CL | clearance (apparent CL/F for extravascular input) | [D]/([T]·[C]) |
| k | elimination rate constant, k = CL/V | 1/[T] |
| ka | first-order absorption rate constant | 1/[T] |
| tlag | lag time before absorption or infusion starts | [T] |
| T | duration of a zero-order input (infusion or zero-order absorption) | [T] |
| R0 | rate of a zero-order input, R0 = D/T | [D]/[T] |
| C(t) | concentration in the central compartment, amount divided by V | [C] |
| A | amount of drug in the central compartment (C = A/V) | [D] |

## 2. General conventions

### MOD-GEN-01 The system
Central compartment with amount A(t) and linear first-order elimination: dA/dt = input(t) − k·A, C = A/V. The first-order absorption models add a depot compartment with amount G: dG/dt = −ka·G, input(t) = ka·G. The zero-order models use a constant input rate over a fixed duration.
- Status: `confirmed by oracle`
- Oracle: all six models, 21 cases, concentration and AUC against the matrix exponential and the adaptive ODE solution (E-08).
- Sources: S-20 (the same catalogue of six single-dose one-compartment models); S-21 (the manual writes the oral theophylline model as a depot and a central compartment, concentration = central amount over volume); S-17, S-18 (textbook chapters on the one-compartment model, book level only); D.

### MOD-GEN-02 Time origin and dose time
All formulas take t = time since the dose. A time before the dose (t < 0) gives C = 0, also for the IV bolus (the drug is not there yet). A point exactly at t = 0 is evaluated by the formula (IV bolus: D/V; extravascular: 0). Lag times add to the dose time: the effective time is t − tlag.
- Status: `confirmed by oracle`
- Oracle: every case includes t = −1, t = 0 and the lag and duration boundaries (the grid has 16 to 18 times), concentration 0 before the dose and before the lag, the infusion boundary continuous; unsorted times are accepted (E-08).
- Sources: design choice, consistent with `specs/nca.md` NCA-DAT-01 (samples before the dose are not used).

### MOD-GEN-03 Parameterisation and bioavailability
The primary parameters are V and CL; k = CL/V is derived. V and k are an equivalent pair (CL = V·k). The user gives exactly one of CL and k (both, or neither, is an error); results show both. For extravascular input the bioavailability F multiplies the dose: only F·D is observable from concentrations, so V and CL are the apparent V/F and CL/F, and the model takes the effective dose D_eff = F·D with F fixed (default 1) and never estimated together with V and CL. The unit of V/F is [D]/[C], the unit of CL/F is [D]/([T]·[C]) (`specs/nca.md` NCA-UNIT-01).
- Status: `confirmed by oracle`
- Oracle: V with CL (`model_iv_bolus_cl`, `model_oral_1`, …) and V with k (`model_iv_bolus_k`, `model_oral_1_k`, …) give the same values; the dose given to the model is the effective dose (E-08).
- Sources: S-20 (parameters CL and V, dose, ka, tlag, duration); S-21; S-01:pk.calc.cl and pk.calc.vz in the `specs/nca.md` sense (apparent quantities); D (F and V enter the equations only as F·D/V).

### MOD-GEN-04 Parameter domains and errors
V > 0, CL > 0 (so k > 0), ka > 0, T > 0, tlag ≥ 0, D > 0 are required, all finite. A parameter outside its domain is a readable error naming the parameter and the value, raised before any evaluation. The derived k or CL must be finite and > 0 as well. A parameter that does not belong to the model, a missing required parameter, or both CL and k are errors naming the parameter. A dose of 0 is allowed and gives C = 0 and AUC 0 everywhere (useful for a placebo or a washout record). A time that is NaN or infinite is an error. A time of +∞ is not evaluated.
- Status: `confirmed by oracle`
- Oracle: the error cases of `oracle_models.rs` (unknown parameter names, wrong parameter sets, out-of-domain values, readable messages) and `model_iv_bolus_zero_dose`. These tests check that the engine refuses; they are specification tests, not numeric references (E-08, E-09).
- Sources: golden rule 6 of `AGENTS.md` (never crash, readable errors); design choice.

### MOD-GEN-05 Model catalogue and ids
Models are chosen by route and number of compartments (AGENTS.md section 7, friction 6). Stable ids:

| model id | route | input | parameters | equation |
|---|---|---|---|---|
| `pk1.iv_bolus` | IV | bolus | V, CL (or k) | MOD-IVB-01 |
| `pk1.iv_infusion` | IV | zero-order, duration T | V, CL (or k), T | MOD-IVI-01 |
| `pk1.oral_1` | extravascular | first-order, no lag | V/F, CL/F (or k), ka | MOD-AB1-01 |
| `pk1.oral_1_lag` | extravascular | first-order with lag | V/F, CL/F (or k), ka, tlag | MOD-AB1-05 |
| `pk1.oral_0` | extravascular | zero-order, no lag | V/F, CL/F (or k), T | MOD-AB0-01 |
| `pk1.oral_0_lag` | extravascular | zero-order with lag | V/F, CL/F (or k), T, tlag | MOD-AB0-02 |

The prefix `pk1` means one compartment; `pk2` (two compartments) is specified in section 11 (MOD-2C-20). The six ids coincide one to one with the six single-dose functions of S-20.
- Status: `confirmed by oracle`
- Oracle: the six ids are the model ids of the 21 cases (`pk1.iv_bolus`, `pk1.iv_infusion`, `pk1.oral_1`, `pk1.oral_1_lag`, `pk1.oral_0`, `pk1.oral_0_lag`), E-08.
- Sources: S-20 (catalogue); `AGENTS.md` sections 4 and 7.

## 3. Intravenous bolus

### MOD-IVB-01 Concentration
C(t) = (D/V)·e^(−k·t) for t ≥ 0. The initial concentration is C0 = D/V. With CL and V: C(t) = (D/V)·exp(−(CL/V)·t).
- Status: `confirmed by oracle`
- Oracle: `model_iv_bolus_cl`, `model_iv_bolus_k`, `model_iv_bolus_b` (E-08).
- Sources: S-17, S-18 (book level); S-20; D (solution of dA/dt = −kA, A(0) = D). Checked in worked example M1.

### MOD-IVB-02 Derived quantities
Half-life t½ = ln 2/k. AUC from 0 to infinity = D/CL = C0/k; AUC(0, t) = (D/CL)·(1 − e^(−k·t)). AUMC to infinity = D/(V·k²) = C0/k². MRT = 1/k. Vss = V. Tmax and Cmax: not defined (the maximum is C0 at t = 0).
- Status: `confirmed by oracle`
- Oracle: AUC(0, t), AUC to infinity, half-life, MRT, Vss and C0 in the same cases (E-08).
- Sources: S-17, S-18; `specs/nca.md` NCA-EXT-04 to EXT-07 and W2 (the same relations in NCA, MRT = 5 and Vss = 10 for k = 0.2, V = 10); D (integrals of e^(−kt) and t·e^(−kt)).

## 4. Intravenous infusion

### MOD-IVI-01 Concentration during and after the infusion
Let R0 = D/T. For 0 ≤ t ≤ T: C(t) = (R0/CL)·(1 − e^(−k·t)). For t > T: C(t) = C(T)·e^(−k·(t − T)) with C(T) = (R0/CL)·(1 − e^(−k·T)). Equivalently, for t > T, C(t) = (R0/CL)·(1 − e^(−k·T))·e^(−k·(t − T)). The two branches agree at t = T, and the infusion reduces to the bolus when T → 0 with D fixed.
- Status: `confirmed by oracle`
- Oracle: `model_iv_infusion`, `model_iv_infusion_b`, with times before, during, at the end of and after the infusion (E-08).
- Sources: S-17, S-18; S-20 (the infusion function takes dose, CL, V and the infusion duration); D (solution of dA/dt = R0 − kA for t ≤ T, then free decay). Checked in worked example M3 and by numerical integration (H1).

### MOD-IVI-02 Derived quantities
Cmax = C(T) at Tmax = T (the concentration rises during the infusion and falls after it). AUC to infinity = D/CL. AUC(0, T) = (R0/CL)·(T − (1 − e^(−k·T))/k) and AUC(T, ∞) = C(T)/k, which add up to D/CL. MRT (the IV form of `specs/nca.md` NCA-EXT-04) = 1/k + T/2 for the profile moment ratio AUMC/AUC; the disposition MRT of the system is 1/k. Vss = V.
- Status: `confirmed by oracle`
- Oracle: Cmax and Tmax = T, AUC(0, t) across the infusion boundary, AUC to infinity, MRT and Vss in the same cases (E-08).
- Sources: `specs/nca.md` NCA-EXT-04, EXT-07 and W5 (AUMC/AUC = 1/k + T_inf/2); S-01:pk.calc.mrt; D.

## 5. First-order absorption

### MOD-AB1-01 Concentration without lag
For ka ≠ k and t ≥ 0: C(t) = D·ka/(V·(ka − k)) · (e^(−k·t) − e^(−ka·t)). With D the effective dose F·D and V, CL the apparent quantities (MOD-GEN-03). C(0) = 0.
- Status: `confirmed by oracle`
- Oracle: `model_oral_1`, `model_oral_1_k`, `model_oral_1_fast_ka` (ka/k = 500), `model_oral_1_slow_ka`, `model_oral_1_lag`, `model_oral_1_lag_b`, E-08.
- Sources: S-17, S-18 (the Bateman function, book level); S-20; D (solution of the depot–central pair of equations). Checked in worked example M2 and by numerical integration (H1).

### MOD-AB1-02 The limit ka = k
For ka = k: C(t) = D·k·t·e^(−k·t)/V. This is the continuous limit of MOD-AB1-01 as ka → k (D: the difference quotient (e^(−kt) − e^(−ka·t))/(ka − k) tends to t·e^(−kt)).
- Status: `confirmed by oracle`
- Oracle: `model_oral_1_ka_eq_k`, `model_oral_1_lag_ka_eq_k` (analytic limit in the oracle), E-08.
- Sources: D; worked example M5.

### MOD-AB1-03 Evaluation near ka = k, and other stable forms
Evaluating MOD-AB1-01 as written loses digits when |ka − k| is small relative to k. The oracle's near-degenerate cases (ka = k + 1e-3, 1e-6, 1e-9) are held to 1e-12 relative, which the textbook form cannot reach, so the engine evaluates equivalent rearrangements that have no positive exponent and no difference of nearly equal numbers. With a = min(k, ka), δ = |ka − k|, s the time since the start of the input and g(z) = (1 − e^(−z))/z (g(0) = 1, computed through `expm1`):
- first-order input, symmetric in k and ka: C(s) = (D/V)·ka·s·e^(−a·s)·g(δ·s) (D: e^(−k·s) − e^(−ka·s) = ±e^(−a·s)·(1 − e^(−δ·s)) and ka/(ka − k) cancels the δ);
- its area: AUC(0, s) = (D/CL)·[ q(a·s) + a·s·e^(−a·s)·(1 − g(δ·s)) ] with q(y) = 1 − e^(−y)·(1 + y); both terms are positive and 1 − g(z) is evaluated by its alternating series below z = 0.1, so there is no cancellation for any ka/k, including ka = k (then 1 − g = 0 and AUC = (D/CL)·q(k·s));
- zero-order input and infusion: C(s) = (D/(T·CL))·(−expm1(−k·s)) for s ≤ T, AUC during the input = (D/(T·CL))·s·(1 − g(k·s)), after it AUC(T) + C(T)·(−expm1(−k·(s − T)))/k; bolus AUC = (D/CL)·(−expm1(−k·t));
- Tmax of the first-order model: ln(ka/k)/(ka − k) when ka/k < 0.5, ln(1 + (ka − k)/k)/(ka − k) (through `ln_1p`) near 1, 1/k when equal, so that ka ≪ k (ka/k down to 1e-17) neither loses digits nor overflows; Cmax = (D/V)·e^(−k·Tmax).

These are exact rewritings of MOD-AB1-01/02/04 and MOD-IVI-01; worked example M5 gives values for them. The values above were checked by the reader in 60-digit arithmetic against the textbook forms (hand check H8).
- Status: `confirmed by oracle`
- Oracle: `model_oral_1_ka_near_k_1e3`, `model_oral_1_ka_near_k_1e6`, `model_oral_1_ka_near_k_1e9` (concentration and AUC at 1e-12 relative), `model_oral_1_ka_eq_k`, `model_oral_1_fast_ka`, `model_oral_1_slow_ka` (E-08, E-09).
- Sources: design choice for the forms (golden rule 6: no NaN from 0/0); D; E-08, E-09. The textbook AUC of MOD-AB1-04 does not survive ka − k = 1e-9 at that tolerance (card T-009 open item 1), which is why the rewriting above is needed.

### MOD-AB1-04 Tmax, Cmax and AUC
For ka ≠ k: Tmax = ln(ka/k)/(ka − k); Cmax = (D/V)·e^(−k·Tmax) = (D/V)·(k/ka)^(k/(ka − k)). For ka = k: Tmax = 1/k and Cmax = D/(V·e). AUC from 0 to infinity = D/CL = D/(V·k) (independent of ka). AUC(0, t) = D/(V·(ka − k)) · ( (ka/k)·(1 − e^(−k·t)) − (1 − e^(−ka·t)) ). With a lag, Tmax is increased by tlag and Cmax is unchanged.
- Status: `confirmed by oracle`
- Oracle: Cmax, Tmax (`cmax_pred`, `tmax_pred`), AUC(0, t) and AUC to infinity in the first-order cases above, including ka ≪ k (`model_oral_1_slow_ka`) and ka ≫ k (`model_oral_1_fast_ka`); with a lag Tmax is increased by tlag (`model_oral_1_lag`), E-08.
- Sources: S-17, S-18 (book level); D (set dC/dt = 0, use ka·e^(−ka·Tmax) = k·e^(−k·Tmax); integrate the two exponentials); worked example M2 (AUC to infinity = 50 for D = 100, CL = 2).

### MOD-AB1-05 Lag time
With a lag, C(t) = 0 for t ≤ tlag and C(t) = the lag-free function of MOD-AB1-01/02 evaluated at t − tlag for t > tlag. The function is continuous at tlag (it is 0 on both sides) but has a kink there.
- Status: `confirmed by oracle`
- Oracle: `model_oral_1_lag`, `model_oral_1_lag_b`, `model_oral_1_lag_ka_eq_k`: 0 up to and at the lag, formula at t − tlag after it (E-08).
- Sources: S-20 (separate lag variants of the first-order and zero-order models); S-21; D.

### MOD-AB1-06 Flip-flop
The concentration function is unchanged when the roles of ka and k are exchanged and V is replaced by V·k/ka (D: after the exchange the prefactor is D·k/(V'·(k − ka)) and the bracket is e^(−ka·t) − e^(−k·t), whose sign change cancels that of (k − ka); the two functions are equal exactly when k/V' = ka/V). Data alone then cannot tell "fast absorption and slow elimination" from "slow absorption and fast elimination". The model definition requires nothing; fitting and initial estimates assume ka > k by default (`specs/fit.md` FIT-INI-03) and warn that the other solution exists. Worked example M4 gives the pair.
- Status: `confirmed by oracle`
- Oracle: `model_oral_1_flip_flop` (the partner of worked example M4 gives the same concentrations as `model_oral_1`), E-08.
- Sources: S-17, S-18 (flip-flop kinetics, book level); D; H2.

## 6. Zero-order absorption

### MOD-AB0-01 Concentration without lag
A constant input at rate R0 = D/T for 0 ≤ t ≤ T into the central compartment with first-order elimination. The concentration is the one of MOD-IVI-01 with D the effective dose F·D: for 0 ≤ t ≤ T, C(t) = (R0/CL)·(1 − e^(−k·t)); for t > T, C(t) = C(T)·e^(−k·(t − T)). The only difference from an IV infusion is the meaning of the parameters (V/F, CL/F apparent, T the absorption duration).
- Status: `confirmed by oracle`
- Oracle: `model_oral_0` (and `model_iv_infusion`, same function), E-08.
- Sources: S-20 (zero-order oral function with parameter `dur`, the absorption duration, same V and CL); D.

### MOD-AB0-02 With lag
Shift as in MOD-AB1-05: C(t) = 0 for t ≤ tlag; for t > tlag use MOD-AB0-01 at t − tlag. Tmax = tlag + T and Cmax = C(tlag + T).
- Status: `confirmed by oracle`
- Oracle: `model_oral_0_lag`, `model_oral_0_lag_b`: Tmax = tlag + T (E-08).
- Sources: S-20; D; worked example M3 (lag variant).

### MOD-AB0-03 Rate or duration
The user may state the input as a duration T (default) or as a rate R0 with T = D/R0. Only one of the two is a parameter; the other is derived. Fitting uses the duration (a positive quantity with a natural bound). Status of the implementation: the engine takes `dur` only; the rate form `rate` is not a model parameter yet.
- Status: `assumed`
- Sources: design choice; S-20 (duration is the parameter of the catalogued functions).

## 7. Secondary parameters

### MOD-SEC-01 Half-life
t½ = ln 2/k.
- Status: `confirmed by oracle`
- Oracle: `half_life` in every case (E-08).
- Sources: S-17, S-18; `specs/nca.md` NCA-LZ-10 (same formula for λz); D.

### MOD-SEC-02 List per model
Reported when the model has them (value, unit, and a standard error in the fit, `specs/fit.md` FIT-OUT-06): k, t½, CL, V, AUC to infinity (= D/CL), Cmax and Tmax (analytic where they exist: oral first-order and zero-order, infusion), C0 (IV bolus), MRT (system, 1/k), MRT of the profile (1/k + 1/ka, plus tlag, for first-order oral; 1/k + T/2, plus tlag, for zero-order input), Vss (= V). The mean absorption time is 1/ka for first-order input and T/2 for zero-order input.
- Status: `confirmed by oracle`
- Oracle: `k`, `v`, `cl`, `half_life`, `auc_inf`, `cmax_pred`, `tmax_pred`, `mrt` (profile MRT, with the absorption time, the lag and half the zero-order duration), `mrt_system` (= 1/k), `vss` in the 21 cases; checked against the ODE solution's integrated first moment (E-08). Not covered by the oracle: C0 as a separate name (it is the concentration at t = 0 of the IV bolus case).
- Sources: `specs/nca.md` NCA-EXT-04 (MRT with the infusion half-duration); S-17, S-18; D (statistical moments: each first-order stage adds its mean transit time). Worked example M2 (MRT = 6 for ka = 1, k = 0.2).

### MOD-SEC-03 Consistency with NCA
For data lying exactly on a model, the NCA of `specs/nca.md` and the analytic values agree when the sampling is dense and extends far enough: AUC to infinity, λz = k, half-life, Cmax and Tmax of the grid, CL, Vz and Vss (IV). The checks are test properties (independent computation, AGENTS.md section 5), with the tolerance of the exponential-trapezoid error of NCA-AUC-10 for sparse grids.
- Status: `documented, untested`
- Sources: `specs/nca.md` W2 (the exact mono-exponential, same numbers as example M1 here: AUC to 12 h = 45.4641023355, AUC to infinity = 50); D.

## 8. Numerics, later work, vocabulary

### MOD-NUM-01 Overflow and underflow
e^(−a·s) with a·s large underflows to 0 and is correct as 0. The engine forms the products s·e^(−a·s) and a·s·e^(−a·s) first and sets them to 0 once the exponential underflows (and q(y) = 1 once y is infinite), so that a large time or rate never gives infinity times zero: a concentration at t = 1e308, or with k and ka near 1e10 and t = 1e300, is 0 and a finite AUC. The rewritings of MOD-AB1-03 have no cancellation, so no result is negative by rounding and no clamp to 0 is needed except before the lag, where the value is 0 by definition.
- Status: `assumed`
- Sources: design choice (golden rule 6); reviewer findings on card T-010 (E-09). Not exercised by the oracle (its grids are ordinary); covered by engine unit tests.

### MOD-NUM-02 Non-finite results are an error
If valid-looking input produces a non-finite concentration, AUC or secondary parameter (for example V = 1e-300 with a large dose, or CL = 1e-320), the run returns an `Overflow` error naming the first offending quantity and telling the user to rescale V, CL or the dose. It is never a silent infinity or NaN, and an output with an infinity is not produced (it could not be serialised as JSON).
- Status: `assumed`
- Sources: golden rule 6; reviewer findings on card T-010 (E-09).

### MOD-MD-01 Multiple dosing (later)
For these linear models the profile for a dose schedule {(t_j, D_j)} is the sum of single-dose profiles, each evaluated at t − t_j (superposition). The steady-state profile of repeated equal doses at interval τ multiplies the single-dose terms by 1/(1 − e^(−k·τ)) for the elimination exponential and 1/(1 − e^(−ka·τ)) for the absorption exponential. Not specified further; a later card.
- Status: `documented, untested`
- Sources: S-17, S-18 (superposition, book level); S-20 (the catalogue also has steady-state functions); D.

### MOD-VOC-01 Vocabulary
Proposed ids are the user-facing names for the CLI, the MCP server, the UI and exports. The engine looks up parameters by these names.

| proposed id | meaning | alias used by other tools (S-20, S-21) | unit |
|---|---|---|---|
| `v` | volume (apparent V/F for extravascular input) | V | [D]/[C] |
| `cl` | clearance (CL/F) | CL | [D]/([T]·[C]) |
| `k` | elimination rate constant | ke, kel | 1/[T] |
| `ka` | first-order absorption rate constant | ka | 1/[T] |
| `tlag` | lag time | tlag, ALAG | [T] |
| `dur` | duration of a zero-order input | dur, tinf | [T] |
| `rate` | zero-order input rate (derived; not a model parameter in the engine yet) | R0 | [D]/[T] |
| `f` | bioavailability (fixed, never fitted with V and CL; the engine takes the effective dose instead) | F, Favail | none |
| `dose` | dose | dose, DOSE | [D] |
| `half_life` | ln 2/k | t½ | [T] |
| `c0` | D/V, IV bolus | C0 | [C] |
| `cmax_pred`, `tmax_pred` | analytic peak concentration and time | | [C], [T] |
| `mrt_system` | 1/k, the mean residence time of the system | | [T] |
| `auc_inf` | D/CL | | [T]·[C] |
| `mrt` | mean residence time of the profile, including the absorption time, the lag and half the zero-order duration (MOD-SEC-02) | | [T] |
| `vss` | V for one compartment | | [D]/[C] |

The suffix `_pred` separates the model-based Cmax and Tmax from the observed `cmax` and `tmax` of `specs/nca.md` section 10. The parameter names `v`, `cl`, `k`, `ka`, `tlag`, `dur` and the scalar names above are the ones used by the oracle files and the engine.
- Status: `assumed`
- Sources: S-20, S-21 (aliases); `specs/nca.md` section 10 (naming style); `AGENTS.md` section 7.

## 9. Worked examples and checks

Numbers were computed by the reader in a throwaway script kept outside the repository (hand checks H, to the digits shown) and are targets for unit tests. In every example D = 100, V = 10, CL = 2, k = 0.2.

**H8. Stable forms against 60-digit arithmetic (T-017).** The reader re-evaluated the textbook concentration at 60 digits for the three neighbours of M5, and the stable AUC form of MOD-AB1-03 at 60 digits against the textbook AUC (ka − k = 1e-3, 1e-6, 1e-9, t = 5): the two agree to all printed digits.

**H1. Closed forms against numerical integration.** The reader integrated the differential equations of each model with a fourth-order Runge–Kutta scheme (step 1e-4) and compared with the closed forms at the times used below: agreement to better than 1e-12 relative for the oral model with lag (value 6.2378070966 at 3 h after the start of absorption, ka = 1) and the infusion model (8.2419988491 at the end of the 2 h infusion; 4.5233048731 at 5 h), and the trapezoidal sum of the oral concentration function over 0 to 6 h (600 000 intervals) reproduces the closed-form AUC(0, 6) = 31.2063461577 to 1e-10.

**M1. IV bolus (`pk1.iv_bolus`).** C0 = 10; C(1) = 10·e^(−0.2) = 8.1873075308; C(4) = 10·e^(−0.8) = 4.4932896412; C(12) = 0.9071795329. t½ = ln 2/0.2 = 3.4657359028. AUC to infinity = 100/2 = 50; AUC(0, 4) = 50·(1 − e^(−0.8)) = 27.5335517941; AUC(0, 12) = 50·(1 − e^(−2.4)) = 45.4641023355 (the same number as the NCA worked example W2). AUMC to infinity = 100/(10·0.04) = 250 and MRT = 5.

**M2. First-order absorption, ka = 1 (`pk1.oral_1`).** The prefactor is D·ka/(V·(ka − k)) = 100/(10·0.8) = 12.5, so C(t) = 12.5·(e^(−0.2t) − e^(−t)). C(0.5) = 12.5·(0.9048374180 − 0.6065306597) = 3.7288344790; C(1) = 5.6356413988; C(2) = 12.5·(0.6703200460 − 0.1353352832) = 6.6873095350; C(6) = 3.7339432467; C(12) = 1.1338976135. Tmax = ln 5/0.8 = 2.0117973905; Cmax = 10·e^(−0.2·2.0117973905) = 6.6874030498 (also 10·(0.2)^(0.25) = 6.6874030498: here k/(ka − k) = 0.25). AUC(0, 6) = 100/(10·0.8)·( 5·(1 − e^(−1.2)) − (1 − e^(−6)) ) = 31.2063461577; AUC to infinity = 50. MRT of the profile = 1/0.2 + 1/1 = 6. With tlag = 0.5, C(1.5) = C(1) of the lag-free model = 5.6356413988, C(0.5) = 0 and Tmax = 2.5117973905 with the same Cmax.

**M3. Infusion and zero-order absorption with lag (`pk1.iv_infusion`, `pk1.oral_0_lag`).** T = 2, R0 = 50, R0/CL = 25. C(1) = 25·(1 − e^(−0.2)) = 4.5317311731. C(2) = 25·(1 − e^(−0.4)) = 8.2419988491 = Cmax at Tmax = 2. C(5) = 8.2419988491·e^(−0.6) = 4.5233048731. AUC(0, 2) = 25·(2 − 0.3296799540/0.2) = 8.7900057545; AUC(2, ∞) = 8.2419988491/0.2 = 41.2099942455; the sum is 50. MRT of the profile = 1/0.2 + 2/2 = 6 (the NCA identity W5 with AUMC/AUC = 6). The same numbers describe `pk1.oral_0` with the apparent V/F and CL/F. With tlag = 0.5 (`pk1.oral_0_lag`): C(3) = C(2.5 without lag) = 8.2419988491·e^(−0.1) = 7.4576689581, and Tmax = 2.5.

**M4. Flip-flop pair.** (ka = 1, k = 0.2, V = 10) and (ka' = 0.2, k' = 1, V' = V·k/ka = 2, i.e. the roles exchanged) give the same concentrations: C(1) = 5.6356413988 and C(3) = 6.2378070966 for both. In the second set the declining phase is governed by the "absorption" constant; the NCA terminal slope 0.2 is then ka', not the elimination rate constant 1.

**M5. The case ka = k and its neighbours (corrected by T-017).** For ka = k = 0.2: C(5) = 100·0.2·5·e^(−1)/10 = 3.6787944117. For ka = 0.2 + 1e-3, 1e-6, 1e-9 the exact values (60-digit arithmetic, H8; the oracle's 256-bit values agree) are C(5) = 3.6879607985, 3.6788036087 and **3.6787944209**. The earlier text of this example printed 3.6787944006 for the last case: that number is what a direct double-precision evaluation of the textbook form returns, damaged by cancellation (it subtracts two numbers equal to about 1e-9 relative and divides by 1e-9, keeping about seven of sixteen digits), and is not the value of the function; it is wrong by 2.0e-8 absolute. The value tends to the limit linearly in ka − k (slope about 9.2 per unit of ka − k at t = 5), which is how the three neighbours and the limit fit together: 3.6787944117 + 9.2e-9 = 3.6787944209. The same cases give AUC(0, 5) = 13.2579642672 (ka − k = 1e-3), 13.2121018677 (1e-6), 13.2120559288 (1e-9) and 13.2120558829 for ka = k (= 50·(1 − 2/e)); the form of MOD-AB1-03 reproduces all of them to 1e-12 relative, the textbook AUC does not.

## 10. Open items

| id | item | rule | who and how |
|---|---|---|---|
| OM-01 | CLOSED 2026-10-08 (T-009, T-010): the model oracle exists (21 cases, 886 values, independent 256-bit implementation, ODE cross-checks, 73 tests green). Not covered: textbook examples with page references, two-compartment models, multiple dosing, extreme parameter ranges (overflow handling is covered by engine unit tests only) | all | later cards |
| OM-02 | Which models and which parameter names the reference software offers are not known to the reader (documentation is off limits); the reader's catalogue follows two open packages (S-20, S-21). Compare names with the headers of the human's exports | MOD-GEN-05, MOD-VOC-01 | question Q-009 |
| OM-03 | Whether the reference software's zero-order absorption has its own "lag" or treats lag separately, and whether its first-order model with ka = k is accepted | MOD-AB1-02, MOD-AB0-02 | private oracle / human's screen |
| OM-04 | Two-compartment models: specified in section 11 (T-031), oracle T-032, engine T-033, spec synced by T-035; open items OM-08 to OM-14 there. Michaelis–Menten elimination, multiple dosing and steady state are not specified | MOD-MD-01 | later cards |
| OM-05 | Textbook section and page numbers for the one-compartment equations | all | someone holding the books (S-17, S-18) |

## 11. Two-compartment models (cards T-031, T-035)

Written by the `reader` agent on 2026-10-09, in its own words and formulas, before any oracle or engine work for two compartments (marching order, `AGENTS.md` section 12 step 7), and synced the same day with the oracle (card T-032) and the engine (card T-033) by card T-035. Ids: `MOD-2C-nn`. Sources are cited by the keys of `specs/sources.md` (the keys added for this section are S-33 to S-35, in its section 11; S-13, S-14, S-15, S-17, S-18, S-20, S-30 are older keys). A rule is `confirmed by oracle` when the cases named in its line `Oracle:` exercise it and the engine reproduces them (see the next paragraph). A rule is `documented, untested` when it is a standard result of the textbooks (book level, S-17, S-18) or papers (S-33, S-34), re-derived (D) and, where stated, checked by the reader (hand checks H11 to H14, section 11.7); a choice made for Caladrius is `assumed`. The reference software's own documentation was not used (Q-005 pending).

**The two-compartment oracle (card T-032) and the engine (card T-033).** Directory `oracle/expected/models/pk2/` (a subdirectory of the one-compartment cases). Three kinds of case: 107 value cases `model_pk2_<id>_<case>` (11 145 values: concentration, AUC and AUMC at every time of the grid, and about 25 scalars), 64 derivative cases `model_pk2_deriv_<id>_<case>` (5 681 values) and one error suite `model_pk2_errors` (80 refusals in six groups: `domain`, `one_compartment`, `macro`, `sets`, `numeric`, `times`). `<id>` is one of `iv_bolus` (24 value cases), `iv_infusion` (17), `oral_1` (33), `oral_1_lag` (10), `oral_0` (15), `oral_0_lag` (8). The expected values come from an independent evaluation of the explicit sums of exponentials at 256 bits, rounded once to double, and are cross-checked on every case within 1e-8 by a matrix exponential and by an adaptive ODE solution that also integrates the area and the first moment; the exponents are the roots of the textbook quadratic and the weights the textbook quotient, so the oracle never uses the engine's stable forms (MOD-2C-21, 25). The engine passes all 508 tests of `oracle_models_2c` (428 value tests, 64 derivative tests, 6 error groups, 9 property tests, 1 guard) and all 17 792 model values of the conformance run (886 one-compartment, 11 145 + 5 681 two-compartment values and the 80 refusals). Tolerances: `MODEL_VALUES` (1e-12 relative) for values and scalars, `MODEL_DERIVATIVES` (also 1e-12 relative, absolute 0, so an expected exact 0 must be returned as exactly 0) for derivatives. In the lines `Oracle:` below the case names are written without the `model_pk2_` prefix; `<id>` stands for the ids named in the line. Families of cases (suffixes): `base` (the point of the worked example N1: cl 2, vc 10, q 4, vp 8, dose 100), `distribution` (k12 much larger than k10), `small_exchange`, `ab_ratio_1e3` (alpha/beta = 1e3), `near_degenerate_1e3` and `near_degenerate_1e6` (clearance set), `near_degenerate_1e9_micro`, `near_degenerate_1e12_micro`, `k10_ne_k21_1e9_micro`, `k10_ne_k21_1e12_micro` (the micro-constant points of N6), `extreme_fast`, `extreme_slow`, `zero_dose`; the suffixes `_micro` and `_macro` after a family name give the same point in the other two parameter sets; oral families `ka_eq_alpha_macro`, `ka_eq_beta_macro`, `ka_alpha_plus_1e3`, `_1e6`, `_1e9`, `ka_alpha_minus_1e3`, `_1e6`, `_1e9`, the same six for beta, `ka_between`, `ka_below_beta`, `ka_500_alpha`, `ka_eq_k10`, `ka_eq_k12`, `ka_eq_k21`, `near_degenerate_1e9_fast_ka`; infusion families `short_duration` and `long_duration`; lag family `long_lag`.

**Notation added to section 1.** Vc central volume, Vp peripheral volume, CL clearance, Q intercompartmental clearance (for extravascular input all four are the apparent quantities divided by F: volumes and clearances scale with 1/F, rates do not, MOD-GEN-03). Micro-constants k10 = CL/Vc, k12 = Q/Vc, k21 = Q/Vp. S = k10 + k12 + k21 and P = k10·k21. α > β > 0 are the two exponents (the roots of r² − S·r + P = 0). Weights wα, wβ (MOD-2C-03). A and B are the intravenous coefficients A = D·wα/Vc and B = D·wβ/Vc (D the effective dose). The unit-dose response function Φ is defined in MOD-2C-06.

### 11.1 System, parameterisations, conversions

#### MOD-2C-01 The system
Three amounts: depot G (extravascular models only), central A1, peripheral A2. dG/dt = −ka·G; dA1/dt = input(t) − (k10 + k12)·A1 + k21·A2; dA2/dt = k12·A1 − k21·A2; C = A1/Vc. input(t) is the bolus amount at t = 0 (IV bolus), the constant rate R0 = D/T on [0, T] (infusion, zero-order absorption), or ka·G (first-order absorption). Elimination is first order from the central compartment only; the peripheral compartment is a linear exchange, so Vss = Vc + Vp (MOD-2C-13). Single dose, linear, time since dose, lag and effective dose exactly as MOD-GEN-02 and MOD-GEN-03. The model reduces to the one-compartment models of sections 3 to 6 when Q = 0, a case excluded here (MOD-2C-05).
- Status: `confirmed by oracle`
- Oracle: the 107 value cases, whose expected values rest on the ODE cross-check (matrix exponential and adaptive solution of the amounts) as well as on the closed forms; the engine reproduces them at 1e-12 (508 of 508 tests of `oracle_models_2c`). The exclusion of Q = 0 is the error group `one_compartment` (MOD-2C-05).
- Sources: S-17, S-18 (two-compartment open model with elimination from the central compartment, book level); S-33 (n-compartment mammillary model with elimination from the central compartment only, abstract); S-35 (central and peripheral compartments, rates q/vc and q/vp); D.

#### MOD-2C-02 The three parameterisations
Every two-compartment id accepts exactly one complete parameter set (a mixture, or two complete sets, is an error naming the offending parameter):

| set | parameters | note |
|---|---|---|
| clearance (default) | `cl`, `vc`, `q`, `vp` | k10 = cl/vc, k12 = q/vc, k21 = q/vp |
| micro | `k10`, `k12`, `k21`, `vc` | cl = k10·vc, q = k12·vc, vp = vc·k12/k21 |
| macro | `a`, `b`, `alpha`, `beta` (and the dose, which is then required, MOD-2C-05) | `a` = A and `b` = B are the **intravenous** coefficients D·wα/Vc and D·wβ/Vc for every route; vc = D/(a + b) |

Further parameters by id as in MOD-GEN-05 (`ka`, `tlag`, `dur`). The clearance set is the default of the catalogue and of the fit (MOD-2C-17 gives why). The macro coefficients are defined by the intravenous bolus even for the oral ids, so that one definition serves all six ids and the conversions never divide by (ka − α) or (ka − β); the coefficients of the oral profile itself (A_o, B_o of MOD-2C-09) are derived outputs. The reference software's choice for oral macro models is not known (open item OM-08).
- Status: `assumed`
- Exercised by oracle cases (the tag stays `assumed`: a design choice of Caladrius, settled only with OM-08): `<id>_base`, `<id>_base_micro` and `<id>_base_macro` for the six ids and the other points in two or three sets (`distribution`, `small_exchange`, `ab_ratio_1e3`, `near_degenerate_1e6`, and the micro-constant points `near_degenerate_1e9_micro`, `near_degenerate_1e12_micro`, `k10_ne_k21_1e9_micro`, `k10_ne_k21_1e12_micro`); the property `the_three_parameterisations_are_one_model` (one point in three sets gives one curve); the error cases `sets_*` (mixture, two complete sets, a macro name next to the clearance set, a missing parameter, `v` and `k` of one compartment, an unknown name). The scalars `a` and `b` of every case are the intravenous coefficients D·w/vc whatever the route, and the coefficients of the oral profile are separate outputs (`a_oral`, `b_oral`, MOD-2C-20). What stays open is whether the reference software defines its oral macro models in the same way (OM-08, Q-005, Q-009).
- Sources: S-20 (the six single-dose two-compartment functions are parameterised by CL, V1, V2, Q); S-35 (cl, vc, q, vp with k12 = q/vc, k21 = q/vp, and alpha, beta, A, B as derived quantities); S-17, S-18 (micro- and macro-constants, book level); design choice for the names, the default and the macro definition.

#### MOD-2C-03 Micro-constants to exponents and weights
S = k10 + k12 + k21. Δ = S² − 4·k10·k21 = (k10 − k21)² + k12·(k12 + 2·k10 + 2·k21), a sum of non-negative terms, > 0 whenever k12 > 0 (so α > β strictly). r = √Δ. α = (S + r)/2, β = k10·k21/α (not (S − r)/2). Then α + β = S, α·β = k10·k21, and k10 and k21 both lie strictly between β and α (D: the quadratic evaluated at k21 equals −k12·k21 < 0, and at k10 equals −k12·k10 < 0). Weights: wα = (α − k21)/(α − β), wβ = (k21 − β)/(α − β), both in (0, 1), wα + wβ = 1, and (s + k21)/((s + α)(s + β)) = wα/(s + α) + wβ/(s + β) (D: partial fractions of the transfer function of the central compartment). Stable evaluation of the weights: with u = k10 + k12 − k21 and the identity (α − k21)·(k21 − β) = k12·k21, take p = α − k21 = (u + r)/2 and q = k21 − β = k12·k21/p if u ≥ 0, otherwise q = (r − u)/2 and p = k12·k21/q; then wα = p/(p + q) and wβ = q/(p + q). Every operation is a sum of positive numbers, a product or a quotient, so no digits are lost for any k10, k12, k21 (MOD-2C-21). The macro coefficients follow: A = D·wα/Vc, B = D·wβ/Vc, A + B = D/Vc.
- Status: `confirmed by oracle`
- Oracle: the micro and clearance points of all six ids, in particular `iv_bolus_near_degenerate_1e9_micro`, `iv_bolus_near_degenerate_1e12_micro`, `iv_bolus_k10_ne_k21_1e9_micro`, `iv_bolus_k10_ne_k21_1e12_micro` (the points of N6, alpha − beta down to 9e-7), `iv_bolus_near_degenerate_1e3` and `iv_bolus_near_degenerate_1e6`; the scalars `alpha`, `beta`, `w_alpha`, `w_beta`, `a`, `b`, and the exponents against the eigenvalues of the 2 × 2 system wherever alpha and beta differ by more than 1e-3. N1 and N6 are reproduced at the printed digits (testkit `worked_example_n1_iv_bolus`, `worked_example_n6_nearly_equal_exponents`).
- Sources: S-17, S-18 (α and β as the roots of the quadratic with sum k10 + k12 + k21 and product k10·k21, book level); S-33 (coefficients and exponents of the polyexponential equation); D (calculus; H11, H12 and example N7 for numbers).

#### MOD-2C-04 Exponents and weights to micro-constants (the reverse conversion)
Given α > β > 0, wα = A/(A + B) ∈ (0, 1), wβ = 1 − wα = B/(A + B), and the dose D: Vc = D/(A + B); k21 = α·wβ + β·wα (the weighted mean of the two exponents, which lies in (β, α)); k10 = α·β/k21; k12 = wα·wβ·(α − β)²/k21. All are positive products and quotients; the textbook k12 = α + β − k10 − k21 cancels when k12 is small relative to α and is not used. Then CL = k10·Vc, Q = k12·Vc, Vp = Q/k21. The two conversions are mutually inverse: the map between {Vc, k10, k12, k21 > 0} and {A > 0, B > 0, α > β > 0} (D fixed) is a bijection (D: the forward direction MOD-2C-03 gives A, B > 0 and α > β > 0; the backward direction above gives positive micro-constants).
- Status: `confirmed by oracle`
- Oracle: the macro points give the same curves as the other sets: `<id>_base_macro` for the six ids, `iv_bolus_distribution_macro`, `iv_infusion_distribution_macro`, `oral_0_distribution_macro`, `iv_bolus_small_exchange_macro`, `iv_bolus_ab_ratio_1e3_macro`, `iv_bolus_near_degenerate_1e6_macro`, `iv_infusion_near_degenerate_1e6_macro`, `oral_0_near_degenerate_1e6_macro`, `oral_1_ka_eq_alpha_macro`, `oral_1_ka_eq_beta_macro`, `oral_1_lag_ka_eq_alpha_macro`; the conversion in both directions is N7 (testkit `worked_example_n7_conversions_both_ways`); the refusals `macro_alpha_equals_beta`, `macro_alpha_below_beta`, `macro_a_zero`, `macro_b_zero`, `macro_beta_zero`, `macro_dose_zero`.
- Sources: S-17, S-18 (conversion between macro- and micro-constants, book level); S-33; D (from α − k21 = wα·(α − β), k21 − β = wβ·(α − β) and (α − k21)(k21 − β) = k12·k21). Worked example N7.

#### MOD-2C-05 Parameter domains and errors
Required: `cl`, `vc`, `q`, `vp` > 0 (or `k10`, `k12`, `k21`, `vc` > 0, or `a`, `b` > 0 and `alpha` > `beta` > 0), all finite; `ka`, `dur` > 0, `tlag` ≥ 0 and D ≥ 0 as in MOD-GEN-04. Refused with a readable error naming the parameter and the value:
1. `q` = 0 or `vp` = 0 (or `k12` = 0): this is a one-compartment model; the message says to use `pk1.*`. A negative value is the usual domain error.
2. Macro set: `alpha` ≤ `beta` (the larger exponent must be named `alpha`; swapping the two names is the same curve, but the engine does not sort silently), `a` ≤ 0, `b` ≤ 0, or a dose that is 0 or missing (vc = D/(a + b) would be undefined).
3. A derived micro-constant or volume that is not finite and > 0 (for example q/vp overflowing), or a computed r, p or q that is 0 or not finite (reachable only when k12·k21 underflows; error code `DegenerateExponents`).
4. A parameter of another parameterisation or of another model id, a missing one, or two sets together.
Not refused: α and β as close as the double-precision input allows (MOD-2C-21), ka equal or close to α, β, k10, k12 or k21 (MOD-2C-10), k10 = k21. A dose of 0 gives C = 0 everywhere with the clearance and micro sets (as MOD-GEN-04); the conventions at dose 0 are in MOD-2C-13 and 15.
Error kinds, as the engine reports them (names of the variants of the model error type, which a client may match on; each carries the offending name or value, and every message is readable text): seven are new for the two-compartment ids. `OneCompartment` (item 1: names the parameter that is 0 and the model, and says to use `pk1`); `MixedParameterSets` (item 4: names two parameters that belong to different sets, and the model); `NoParameterSet` (item 4: no parameter of any set; the message lists the three sets); `AlphaNotAboveBeta` (item 2: carries both values); `MacroWithoutDose` (item 2); `DerivedOutOfRange` (item 3: names the derived quantity, how it is derived, for example q / vp, and its value); `DegenerateExponents` (item 3: k12·k21 underflows). Everything else reuses the kinds of the one-compartment ids, so that pk2 mirrors pk1 (MOD-GEN-04): `UnknownParameter`, `ParameterNotInModel`, `MissingParameter`, `ParameterOutOfDomain`, `InvalidDose`, `NonFiniteTime` and `Overflow` (for pk2 the Overflow message says that the named quantity is not a finite number). The wording that the oracle's suite requires is in the words listed in `model_pk2_errors.options.json`.
- Status: `confirmed by oracle`
- Oracle: the error suite `model_pk2_errors` (80 refusals) covers items 1 to 4: `one_compartment_q_zero_iv_bolus`, `one_compartment_vp_zero_iv_bolus`, `micro_k12_zero` (the message names the parameter and says `pk1`); `macro_*` (item 2); `numeric_overflow_derived_rate` and `numeric_degenerate_exponents` (item 3); `sets_*` including `sets_unknown_name` (item 4); the domain, NaN and infinity cases of the groups `domain` and `times`. Not refused (the closing paragraph of the rule): the `near_degenerate_*` cases, `oral_1_ka_eq_*`, k10 = k21 (`near_degenerate_1e9_micro`) and the `zero_dose` cases. The words that a message must contain are the oracle's (the name of the offending parameter; `pk1`; `overflow` or `finite` where several wordings are acceptable); the engine mirrors the one-compartment errors of MOD-GEN-04. The kinds are listed below.
- Sources: golden rule 6 of `AGENTS.md`; MOD-GEN-04 (same style); D (the exclusion of k12 = 0 follows from Δ > 0 in MOD-2C-03).

### 11.2 Closed forms

#### MOD-2C-06 Superposition of two one-compartment functions
For every input of this file, C(t) = (1/Vc)·[ wα·Φ(α, t) + wβ·Φ(β, t) ], where Φ(λ, t) is the concentration that the one-compartment model with volume 1, elimination rate constant λ (so CL = λ), the same effective dose D and the same input parameters (ka, tlag, T) gives at time t: the functions of sections 3 to 6 with V = 1 and k = λ. This is exact for the bolus, the infusion, zero-order and first-order absorption, with or without lag, because the input enters through a linear transfer function and (s + k21)/((s + α)(s + β)) = wα/(s + α) + wβ/(s + β) (MOD-2C-03). Consequences used below: every two-compartment function is a positive combination of two one-compartment functions (no cancellation); the AUC, the AUMC, the Cmax bracket and the partial derivatives are the same combinations (MOD-2C-14, 15, 18); the engine may build the six ids from its stable one-compartment kernels (MOD-AB1-03), while the oracle's independent reference is the explicit sums of exponentials given in each rule. Whichever way it is written, the same function results.
- Status: `confirmed by oracle`
- Oracle: every value case: the explicit sums of the oracle (not a superposition) and the engine's combination of its one-compartment kernels agree within 1e-12; the ODE cross-check agrees with both within 1e-8.
- Sources: S-17, S-18 (superposition and Laplace-domain solutions of the two-compartment model, book level); S-33; D (linearity and partial fractions; H11 checks the combination against a numerical ODE solution, H12 against the explicit forms).

#### MOD-2C-07 IV bolus
For t ≥ 0: C(t) = A·e^(−α·t) + B·e^(−β·t) = (D/Vc)·[wα·e^(−α·t) + wβ·e^(−β·t)]. C(0) = A + B = D/Vc. Before the dose C = 0 (MOD-GEN-02).
- Status: `confirmed by oracle`
- Oracle: `iv_bolus_*` (24 cases, among them `iv_bolus_extreme_fast`, `iv_bolus_extreme_slow` and times up to 800 mean lives of beta, where the exact 0 of the underflow is expected); N1 at the printed digits (testkit `worked_example_n1_iv_bolus`).
- Sources: S-17, S-18 (biexponential disposition, book level); S-33 (bolus polyexponential equation); S-34 (body as more than a single compartment, two-exponential decline, 1968); D. Worked example N1; H11.

#### MOD-2C-08 IV infusion, during and after
R0 = D/T. For 0 ≤ t ≤ T: C(t) = (A/(α·T))·(1 − e^(−α·t)) + (B/(β·T))·(1 − e^(−β·t)). For t > T: C(t) = (A/(α·T))·(1 − e^(−α·T))·e^(−α·(t − T)) + (B/(β·T))·(1 − e^(−β·T))·e^(−β·(t − T)), equivalently (A/(α·T))·(e^(α·T) − 1)·e^(−α·t) + (B/(β·T))·(e^(β·T) − 1)·e^(−β·t). The branches agree at t = T. The plateau of a long infusion is A/(α·T) + B/(β·T) = R0/CL. The bolus is the limit T → 0 with D fixed.
- Status: `confirmed by oracle`
- Oracle: `iv_infusion_*` (17 cases, among them `iv_infusion_short_duration`, `iv_infusion_long_duration` and times on and around the end of the infusion); N2 (testkit `worked_example_n2_iv_infusion_and_zero_order`); the property `a_very_short_infusion_is_a_bolus_after_the_end`.
- Sources: S-17, S-18; S-33 (constant-rate infusion, abstract); D (MOD-2C-06 applied to MOD-IVI-01; A/T = R0·wα/Vc). Worked example N2.

#### MOD-2C-09 First-order absorption, no lag, ka ≠ α and ka ≠ β
C(t) = A_o·e^(−α·t) + B_o·e^(−β·t) − (A_o + B_o)·e^(−ka·t), with A_o = ka·A/(ka − α) = (D·ka/Vc)·wα/(ka − α) and B_o = ka·B/(ka − β) = (D·ka/Vc)·wβ/(ka − β). Equivalently, in the form that MOD-2C-06 gives and that the engine evaluates: C(t) = (D·ka/Vc)·[ wα·(e^(−α·t) − e^(−ka·t))/(ka − α) + wβ·(e^(−β·t) − e^(−ka·t))/(ka − β) ]. The coefficient of e^(−ka·t) is −(A_o + B_o), which makes C(0) = 0; in micro-constants it equals (D·ka/Vc)·(k21 − ka)/((α − ka)(β − ka)). The signs of A_o and B_o are those of ka − α and ka − β (with ka > α > β they are + and +; with α > ka > β, A_o < 0 and B_o > 0). The oral-profile coefficients are therefore not the IV ones (MOD-2C-02), and A = A_o·(ka − α)/ka, B = B_o·(ka − β)/ka.
- Status: `confirmed by oracle`
- Oracle: `oral_1_*` (33 cases: `base`, `distribution`, `small_exchange`, `ab_ratio_1e3`, `ka_between`, `ka_below_beta`, `ka_500_alpha`, `ka_eq_k10`, `ka_eq_k12`, `ka_eq_k21`, `extreme_fast`, `extreme_slow` and the cases of MOD-2C-10); the scalars `a_oral` and `b_oral`; N3 (testkit `worked_example_n3_first_order_absorption`).
- Sources: S-17, S-18 (oral dosing of the two-compartment model, three exponentials including the absorption term, book level); S-20 (the oral two-compartment functions exist in the catalogue, equations not written out there); D (partial fractions of ka·(s + k21)/((s + ka)(s + α)(s + β)); H12). Worked example N3.

#### MOD-2C-10 The limits ka = α and ka = β, and stable evaluation
The first form of MOD-2C-09 divides by ka − α and ka − β, is undefined at ka = α or ka = β, and loses digits near them (large terms of opposite sign cancel). The limits, by the confluent partial fractions (D), for t ≥ 0:
- ka = α: C(t) = (D·α/Vc)·[ wα·t·e^(−α·t) + wβ·(e^(−β·t) − e^(−α·t))/(α − β) ];
- ka = β: C(t) = (D·β/Vc)·[ wβ·t·e^(−β·t) + wα·(e^(−β·t) − e^(−α·t))/(α − β) ].
Both are the continuous limits of MOD-2C-09 (D: (e^(−x·t) − e^(−ka·t))/(ka − x) → t·e^(−x·t) as ka → x). The single stable form for every ka is the second form of MOD-2C-09 with each difference quotient written as in MOD-AB1-03: for x ∈ {α, β}, a = min(ka, x), δ = |ka − x|, (e^(−x·t) − e^(−ka·t))/(ka − x) = t·e^(−a·t)·g(δ·t), with g(z) = (1 − e^(−z))/z computed through `expm1` and g(0) = 1. Each term is non-negative, the weights are positive and add to 1, so no cancellation occurs for any ka/α or ka/β, the exact limits included, and the result is never negative by rounding. The same holds when α and β are almost equal (MOD-2C-21). The cases ka = k10, ka = k12 or ka = k21 are not special.
- Status: `confirmed by oracle`
- Oracle: `oral_1_ka_eq_alpha_macro` and `oral_1_ka_eq_beta_macro` (the exact limits, reached from the macro set so that ka equals the exponent in floating point), `oral_1_ka_alpha_plus_1e3`, `_plus_1e6`, `_plus_1e9`, `_minus_1e3`, `_minus_1e6`, `_minus_1e9` and the same six for beta, `oral_1_near_degenerate_1e9_fast_ka`; N4 (testkit `worked_example_n4_the_limits_and_their_neighbours`: the values at ka = alpha, ka = beta and their neighbours at the printed digits); the property `ka_equal_alpha_is_the_limit_of_its_neighbours`.
- Sources: D (limits; H12 and H13 against a 70-digit evaluation); MOD-AB1-02 and MOD-AB1-03 (the one-compartment limit and its stable forms); design choice for the evaluation. Worked example N4 gives values at ka = α, ka = β and their neighbours, and the damage done to the textbook form.

#### MOD-2C-11 Lag time
For the extravascular ids with lag, C(t) = 0 for t ≤ tlag and C(t) = the lag-free function of MOD-2C-09/10 (or MOD-2C-12) evaluated at t − tlag for t > tlag. Continuous at tlag, with a kink, as MOD-AB1-05. The IV ids have no lag.
- Status: `confirmed by oracle`
- Oracle: `oral_1_lag_*` (10 cases, among them `oral_1_lag_long_lag` and `oral_1_lag_ka_eq_alpha_macro`) and `oral_0_lag_*` (8 cases): zero before and at the lag, the shifted function after it, AUC and AUMC counted from the dose time; N5 (testkit `worked_example_n5_lag_and_zero_order_with_lag`); the property `the_lag_shifts_the_curve`.
- Sources: MOD-AB1-05, MOD-AB0-02; S-20 (the catalogue has the lagged first-order and zero-order two-compartment functions); D. Worked example N5.

#### MOD-2C-12 Zero-order absorption
The infusion function of MOD-2C-08 with the apparent Vc/F, CL/F, Q/F, Vp/F and T the absorption duration: C(t) = (A/(α·T))·(1 − e^(−α·t)) + (B/(β·T))·(1 − e^(−β·t)) for 0 ≤ t ≤ T and the decaying sum of MOD-2C-08 after T; with lag, shifted by tlag as MOD-2C-11. Its stable one-compartment kernels are those of MOD-AB1-03 (zero-order input and infusion). Tmax = T (+ tlag) exactly and Cmax = C(T).
- Status: `confirmed by oracle`
- Oracle: `oral_0_*` (15 cases) and `oral_0_lag_*`; the scalars `tmax_pred` = T (+ tlag) and `cmax_pred` = C(T); the property `zero_order_absorption_is_the_infusion_function`; N2 and N5 as above.
- Sources: MOD-AB0-01, MOD-AB0-02; S-20 (the same duration parameter); D. Worked example N5.

### 11.3 Derived quantities

#### MOD-2C-13 Disposition quantities
Distribution half-life t½α = ln 2/α; terminal half-life t½β = ln 2/β (the value that `half_life` reports, as λz in NCA); initial concentration of the bolus C0 = D/Vc = A + B. CL = k10·Vc = D/(A/α + B/β). Vss = Vc + Vp = Vc·(1 + k12/k21) = CL·MRT_system (the Vss of NCA, S-14, S-15). Vz (Vβ) = CL/β = D/(β·AUC∞). Volume of the extrapolated terminal line, V_extrap = D/B = Vc/wβ (the second form is the one the oracle and the engine use, because it stays defined at D = 0, where D/B is 0/0). In general Vc < Vss < Vz < V_extrap (checked, H14: no exception in 200 000 random parameter sets, and none in the 107 oracle cases; not proved here). MRT of the system, MRT_system = Vss/CL = (1/k10)·(1 + k12/k21). The mean residence time of the profile adds the input time: MRT = MRT_system + T/2 (infusion, zero-order absorption), + 1/ka (first-order absorption), plus tlag, as MOD-SEC-02. The Vc, Vp, Vss, Vz and CL of the extravascular ids are the /F quantities. **Dose 0 (conventions of the oracle, followed by the engine):** C, AUC and AUMC are 0 at every time; the quantities that do not contain D keep their values (the exponents, wα and wβ, the half-lives, Vss, Vz, V_extrap = Vc/wβ, MRT_system, and MRT = MRT_system + m by the formula of MOD-2C-14, although AUMC/AUC is 0/0); a, b, c0, AUC(0, ∞) and AUMC(0, ∞) are 0.
- Status: `confirmed by oracle`
- Oracle: the scalars `half_life`, `half_life_alpha`, `vss`, `vz`, `v_extrap`, `auc_inf`, `aumc_inf`, `mrt_system`, `mrt` and (bolus) `c0` of every value case, dose 0 included; the identities of the rule and the order Vc < Vss < Vz < V_extrap hold in all 107 cases (testkit `the_scalars_satisfy_the_identities_of_the_spec`; the reader's H14 check of the order over random sets remains a check, not a proof); N1 at the printed digits.
- Sources: S-17, S-18 (distribution and terminal half-lives, volumes of distribution, book level); S-33 (clearance, volume at steady state, extrapolated volume and half-life from the coefficients and exponents, abstract); S-14, S-15 (Vss by statistical moments, any mode of administration); `specs/nca.md` NCA-EXT-04..07; D. Worked example N1.

#### MOD-2C-14 AUC and AUMC
Bolus: AUC(0, ∞) = A/α + B/β = D/CL; AUMC(0, ∞) = A/α² + B/β² = D·Vss/CL²; AUC(0, t) = (A/α)(1 − e^(−α·t)) + (B/β)(1 − e^(−β·t)). For the other ids the finite-time areas are the combination of MOD-2C-06: AUC(0, t) = wα·AUC₁(α; t) + wβ·AUC₁(β; t), where AUC₁(λ; t) is the one-compartment area of MOD-IVI-02 or MOD-AB1-04 (stable forms in MOD-AB1-03) with V = Vc and k = λ, that is D/(Vc·λ) in place of D/CL. Explicitly for first-order absorption: AUC(0, t) = (D/Vc)·Σ_i (w_i/λ_i)·[1 − (ka·e^(−λ_i·t) − λ_i·e^(−ka·t))/(ka − λ_i)], λ_i ∈ {α, β} with the matching weights. The finite-time first-moment area is defined the same way: AUMC(0, t) = ∫ from 0 to t of s·C(s) ds = (1/Vc)·[ wα·∫₀ᵗ s·Φ(α, s) ds + wβ·∫₀ᵗ s·Φ(β, s) ds ]; for the bolus it is (A/α²)·(1 − e^(−α·t)·(1 + α·t)) + (B/β²)·(1 − e^(−β·t)·(1 + β·t)). Both areas count from the dose time (not from the lag) and are 0 for t ≤ 0 and while C is 0. **Output names:** the finite-time areas are the per-time outputs `auc` and `aumc` (one value per input time, in the order of the times, also read as `auc[i]` and `aumc[i]` for the i-th time; the engine's output has an `aumc()` slice next to `auc()`, serialised only when it is not empty so that the one-compartment output is unchanged), and the infinite-time ones are the scalars `auc_inf` and `aumc_inf`. For every id: AUC(0, ∞) = D/CL, independent of ka, T, tlag and Q; AUMC(0, ∞) = D·Vss/CL² + (D/CL)·m with m = 0 (bolus), T/2 + tlag (infusion, zero-order absorption), 1/ka + tlag (first-order absorption). MRT of the profile = AUMC/AUC = Vss/CL + m.
- Status: `confirmed by oracle`
- Oracle: `conc`, `auc` and `aumc` at every time of every value case (the finite-time AUC and AUMC are exact integrals of the terms in the oracle, cross-checked by the ODE solution that integrates the area and the first moment) and the scalars `auc_inf`, `aumc_inf`, `mrt`, `mrt_system`; the closed forms of the rule are also checked against the integrals inside the oracle script to 2^-180. N1 to N3 at the printed digits.
- Sources: S-17, S-18; S-33; S-13 (statistical moments); `specs/nca.md` NCA-EXT-04..07; D (integrals of e^(−λ·t) and t·e^(−λ·t); mean transit times of input and disposition add). Worked examples N1 to N3.

#### MOD-2C-15 Cmax and Tmax
Closed where the shape is simple: bolus, no interior maximum (Cmax = C0 at t = 0); infusion and zero-order absorption, Tmax = T + tlag and Cmax = C(T) (D: during the input C' > 0, since both combined terms increase; after it C' < 0, since both weights are positive). First-order absorption: numerical. C(t) has exactly one maximum for t > tlag (D: C' is a sum of three exponentials, e^(−ka·t), e^(−α·t), e^(−β·t) or their confluent versions, so it has at most two real zeros; C'(0) = D·ka/Vc > 0 and C' < 0 at large t, so the number of zeros is odd, hence one). Bracket: let x_α = ln(ka/α)/(ka − α) and x_β = ln(ka/β)/(ka − β) be the one-compartment peak times of MOD-AB1-04 (1/α and 1/β at equality); then Tmax − tlag lies in [min(x_α, x_β), max(x_α, x_β)] (D: both one-compartment derivatives are positive before their own peak time and negative after it, and the weights are positive). A safeguarded bracketing root finder (bisection steps guarantee convergence) on C'(t) = 0 inside this bracket, to a relative tolerance on t of 1e-12 (the engine bisects on the sign of C' until the bracket closes to adjacent doubles, which is at least as tight; the loop is bounded), gives Tmax; at the maximum C is flat, so Cmax = C(Tmax) is insensitive to the remaining error. The oracle produces `tmax_pred` and `cmax_pred` from its independent evaluation; Cmax is the primary check, Tmax the secondary one. **Dose 0:** the infusion and zero-order ids keep Tmax = T (+ tlag) with Cmax = 0; for the first-order ids `tmax_pred` and `cmax_pred` are not available, because C is identically 0 and every time is a maximum, so no Tmax is defined (the oracle writes no value and the engine reports none).
- Status: `confirmed by oracle`
- Oracle: zero-order and infusion ids: the scalars `tmax_pred` = T + tlag and `cmax_pred` of the cases of MOD-2C-12; first-order ids: `tmax_pred` and `cmax_pred` of `oral_1_*` and `oral_1_lag_*`, including `oral_1_ka_below_beta`, `oral_1_ka_between` and the equalities of MOD-2C-10, against the oracle's 256-bit evaluation (and, in the oracle's own cross-check at 1e-5, a numerical maximisation); N3 (Tmax 0.9501291546, Cmax 6.1898890498 at the printed digits). The bracket is the reader's reasoning (D); the engine's root finding is in MOD-2C-25.
- Sources: MOD-AB1-04; D (sign changes of exponential sums; H13 for the bracket in worked example N3). Worked example N3.

#### MOD-2C-16 Consistency with NCA
For data lying exactly on a two-compartment model, the NCA of `specs/nca.md` agrees with the analytic values only when the sampling resolves both phases, so the checks are test properties with their own tolerance (independent computation, `AGENTS.md` section 5): (a) AUC(0, ∞) of the log-linear trapezoid on a dense grid tends to D/CL; on a sparse grid the error of the trapezoids on a biexponential is not zero (NCA-AUC-10); (b) λz tends to β only when the window starts late enough that the α-term is negligible, A·e^(−α·t) ≤ ε·B·e^(−β·t), that is t ≥ ln(A/(ε·B))/(α − β) (N1 with ε = 1e-6: t ≥ 15.7); an earlier window overestimates λz and so underestimates t½, Vz and the extrapolated area; (c) CL = D/AUC∞ is the same quantity as the model's CL; Vz (NCA) tends to CL/β and not to Vss; (d) Vss of NCA (from AUMC∞) tends to Vc + Vp, so Vss ≠ Vz is expected and not an error; (e) for the IV bolus the back-extrapolated C0 of NCA tends to A + B only if the first samples are early enough; (f) for the infusion and zero-order ids, MRT_NCA − T/2 tends to Vss/CL (NCA-EXT-04); (g) the dose-normalised quantities of the extravascular ids are the /F values.
- Status: `documented, untested`
- Sources: `specs/nca.md` NCA-LZ-01, NCA-EXT-04..07, NCA-AUC-10, W2, MOD-SEC-03; S-17, S-18 (a biexponential curve has terminal slope β only after the distribution phase, book level); D.

### 11.4 Partial derivatives

#### MOD-2C-17 Internal parameters and the choice of parameterisation
Define the internal parameters ψ = (V, α, β, w) with V = Vc and w = wα (wβ = 1 − w). With Φ(λ, t) of MOD-2C-06 (it includes the dose and the input parameters), C = (1/V)·[ w·Φ(α, t) + (1 − w)·Φ(β, t) ]. Every partial derivative of C with respect to ψ and to the input parameters is a combination of the one-compartment derivatives, which the one-compartment engine already has for k, ka, dur and tlag (`specs/fit.md` FIT-JAC-02). The derivatives with respect to a user parameterisation are the chain rule through ψ (MOD-2C-19). Which set is simplest: the macro set (a, b, alpha, beta) gives the most direct formulas, because C = (a/D)·Φ(α, t) + (b/D)·Φ(β, t) is linear in a and b; the ψ set is the natural intermediate. The clearance set is nevertheless the default for fitting: its parameters are positive, directly interpretable, and free of the ordering constraint α > β that a box-bounded fit cannot express. The engine implements the Jacobian ∂ψ/∂θ for the three sets of MOD-2C-02 and multiplies; forward differences remain the user default and analytic derivatives an option, as in FIT-JAC-01/02.
- Status: `assumed`
- Exercised by oracle cases (the tag stays `assumed`: a design choice of Caladrius, settled only with OM-08): the derivative cases in the three sets (MOD-2C-18, 19) reproduce the chain through the internal parameters. The choice of the clearance set as the default parameterisation of the fit stays a design choice, for the reason given in the rule.
- Sources: D (calculus); `specs/fit.md` FIT-JAC-01, FIT-JAC-02 (what the fit needs); design choice for the default parameterisation. H14 (analytic against 70-digit central differences).

#### MOD-2C-18 Derivatives with respect to ψ and the input parameters
With Φ_α = Φ(α, t), Φ_β = Φ(β, t), Φ_λ = ∂Φ/∂λ and D fixed:
- ∂C/∂V = −C/V.
- ∂C/∂w = (Φ_α − Φ_β)/V.
- ∂C/∂α = (w/V)·Φ_λ(α, t); ∂C/∂β = ((1 − w)/V)·Φ_λ(β, t).
- For an input parameter η ∈ {ka, dur, tlag}: ∂C/∂η = (1/V)·[ w·∂Φ/∂η(α, t) + (1 − w)·∂Φ/∂η(β, t) ].
For the IV bolus Φ = D·e^(−λ·t) and Φ_λ = −t·Φ. With the macro parameters (D fixed): ∂C/∂a = Φ_α/D, ∂C/∂b = Φ_β/D, ∂C/∂alpha = (a/D)·Φ_λ(α, t), ∂C/∂beta = (b/D)·Φ_λ(β, t). `dur` is normally fixed in a fit (FIT-BND-03); the derivative with respect to tlag has a kink at t = tlag (zero before it, the one-sided value after it), as in the one-compartment case.
- Status: `confirmed by oracle`
- Oracle: `deriv_<id>_base_macro` and `deriv_<id>_distribution_macro` for the six ids (columns `d_a`, `d_b`, `d_alpha`, `d_beta`), `deriv_oral_1_ka_eq_alpha_macro`; the input parameters `d_ka`, `d_tlag`, `d_dur` where the id has them, among them `deriv_oral_1_ka_alpha_plus_1e6`, `deriv_oral_1_ka_below_beta`, `deriv_oral_1_ka_between`; zero before the dose and before the lag (exact 0 expected). N8 (testkit `worked_example_n8_partial_derivatives`).
- Sources: D (derivative of a sum of two terms; MOD-2C-06). Worked example N8.

#### MOD-2C-19 Chain rule to the user parameterisations
Write α_θ = ∂α/∂θ, β_θ = ∂β/∂θ, d = α − β.
- Micro set (V, k10, k12, k21): α_θ = (α·S_θ − P_θ)/d and β_θ = −(β·S_θ − P_θ)/d, with S_θ = 1 for the three constants and P_θ = k21 (for k10), 0 (for k12), k10 (for k21). That is ∂α/∂k10 = wα, ∂β/∂k10 = wβ; ∂α/∂k12 = α/d, ∂β/∂k12 = −β/d; ∂α/∂k21 = (α − k10)/d, ∂β/∂k21 = (k10 − β)/d (in every case α_θ + β_θ = S_θ). Weight: ∂w/∂θ = [ (α_θ − ∂k21/∂θ) − w·(α_θ − β_θ) ]/d, with ∂k21/∂θ = 1 for k21 and 0 for the others. V enters only through ∂C/∂V.
- Clearance set (cl, vc, q, vp), through the micro set: ∂k10/∂cl = 1/vc, ∂k10/∂vc = −k10/vc; ∂k12/∂q = 1/vc, ∂k12/∂vc = −k12/vc; ∂k21/∂q = 1/vp, ∂k21/∂vp = −k21/vp; V = vc. With ∂C/∂k_j = ∂C/∂α·α_j + ∂C/∂β·β_j + ∂C/∂w·w_j: ∂C/∂cl = (∂C/∂k10)/vc; ∂C/∂q = (∂C/∂k12)/vc + (∂C/∂k21)/vp; ∂C/∂vp = −k21·(∂C/∂k21)/vp; ∂C/∂vc = ∂C/∂V − [k10·∂C/∂k10 + k12·∂C/∂k12]/vc.
- Macro set: V = D/(a + b), w = a/(a + b); ∂C/∂a and ∂C/∂b are direct (MOD-2C-18) and α, β are parameters.
The differences α − k10 and α − k21 are evaluated stably as in MOD-2C-03 (α − k10 = (u′ + r)/2 with u′ = k12 + k21 − k10 when u′ ≥ 0, otherwise k12·k10/((r − u′)/2)). The derivatives with respect to the micro-constants grow like 1/d when α ≈ β: that is the real sensitivity (the data cannot tell k12 from zero) and is reported as such, not suppressed.
- Status: `confirmed by oracle`
- Oracle: clearance set `d_cl`, `d_vc`, `d_q`, `d_vp` (`deriv_<id>_base`, `_distribution`, `_small_exchange`, `_ab_ratio_1e3`, `_near_degenerate_1e6`) and micro set `d_k10`, `d_k12`, `d_k21`, `d_vc` (`deriv_<id>_base_micro`, `_distribution_micro`, `_near_degenerate_1e6_micro`), against central differences at 256 bits with a relative step of 1e-25; 64 cases and 5 681 values, all reproduced at 1e-12 by the engine's analytic Jacobian (it reports the method `Analytic`, never the forward-difference fallback). Not covered: 393 entries whose condition number exceeds 1e3 (zero crossings and the near-degenerate points, where `d_q`, `d_vp` and the micro-constants are really ill-conditioned; the kept entries have a condition number of at most 909) are omitted by the oracle; the engine returns them all the same, but no test checks them (open item OM-14). Exact zeros: see MOD-2C-25.
- Sources: D (implicit differentiation of r² − S·r + P = 0); H14 (the chain rule reproduces 70-digit central differences in worked example N8). Not a textbook result.

### 11.5 Catalogue, numerics

#### MOD-2C-20 Model ids and defaults
Added to the table of MOD-GEN-05 (the prefix `pk2` was reserved there). The six ids coincide one to one with the six single-dose two-compartment functions of S-20.

| model id | route | input | parameters (default set) | equation |
|---|---|---|---|---|
| `pk2.iv_bolus` | IV | bolus | cl, vc, q, vp | MOD-2C-07 |
| `pk2.iv_infusion` | IV | zero-order, duration T | cl, vc, q, vp, dur | MOD-2C-08 |
| `pk2.oral_1` | extravascular | first-order, no lag | cl, vc, q, vp (all /F), ka | MOD-2C-09, 10 |
| `pk2.oral_1_lag` | extravascular | first-order with lag | cl, vc, q, vp, ka, tlag | MOD-2C-11 |
| `pk2.oral_0` | extravascular | zero-order, no lag | cl, vc, q, vp, dur | MOD-2C-12 |
| `pk2.oral_0_lag` | extravascular | zero-order with lag | cl, vc, q, vp, dur, tlag | MOD-2C-12 |

The default parameterisation is the clearance set for every id; the micro and macro sets are selected by supplying their parameters instead (MOD-2C-02). Results always show all three sets (cl, vc, q, vp; k10, k12, k21; a, b, alpha, beta) plus the quantities of MOD-2C-13 and, where closed, MOD-2C-15. Vocabulary additions to MOD-VOC-01 (same status): parameters `vc`, `vp`, `q`, `k10`, `k12`, `k21`, `a`, `b`, `alpha`, `beta` (aliases V1, V2, Q, CL in S-20, S-35), and outputs `half_life` (= terminal, ln 2/β), `half_life_alpha`, `vss`, `vz`, `v_extrap`, `auc_inf`, `aumc_inf`, `mrt_system`, `mrt`, `c0` (bolus), `cmax_pred`, `tmax_pred`, and, added by T-032/T-033: `alpha`, `beta`, `k10`, `k12`, `k21` and the three sets as scalars; `w_alpha`, `w_beta` (the weights wα and wβ of MOD-2C-03); `a`, `b` (the intravenous coefficients for every id, MOD-2C-02); `a_oral`, `b_oral` (the coefficients A_o and B_o of the oral profile, MOD-2C-09, produced only for the first-order ids and only when ka differs from both α and β by at least 1 % of ka, because they are ill-conditioned or undefined at the equalities; no value is reported otherwise, nor at dose 0); and the per-time outputs `conc`, `auc`, `aumc` (MOD-2C-14). The rule "a and b are the intravenous coefficients" holds for every id, the oral ones included. A one-compartment id is never silently upgraded or downgraded by the number of parameters supplied.
- Status: `assumed`
- Exercised by oracle cases (the tag stays `assumed`: a design choice of Caladrius, settled only with OM-08): the six ids are the model ids of the 107 value cases; the clearance set is the default and the other two are selected by their parameters; the output names listed below are those of the oracle's scalars and tables.
- Sources: S-20 (catalogue, argument names CL, V1, V2, Q, ka, tlag, dur/tinf); S-35 (cl, vc, q, vp); design choice (MOD-GEN-05 style, `AGENTS.md` section 7).

#### MOD-2C-21 Numerics: the discriminant, α ≈ β, and the textbook forms
Three places lose digits in a direct implementation. (1) The discriminant S² − 4·k10·k21 subtracts nearly equal numbers when k12 is small; the sum-of-non-negative-terms form of MOD-2C-03 does not (worked example N6: at k12 = 1e-12 and k10 = k21 = 0.2 the textbook form gives r with a relative error of 1.8e-5). (2) The weights (α − k21)/(α − β) and the reverse k12 = α + β − k10 − k21, repaired by MOD-2C-03/04. (3) The three-exponential oral form near ka = α or ka = β, repaired by MOD-2C-10. With these, α and β may be arbitrarily close, down to the point where k12·k21 underflows (MOD-2C-05 item 3), without cancellation: the combined form is a positive combination, and in the limit it becomes the one-compartment function with k = α ≈ β (N6). The textbook explicit forms are what the oracle's independent implementation evaluates in extended precision (OM-06), never what the engine evaluates. The sensitivities (MOD-2C-19) are not regularised. The devices that the engine finally chose for these three places and for the derivatives are in MOD-2C-25. The fit's own safeguards for α ≈ β (Vp and Q unidentifiable when the data show a single phase) belong to `specs/fit.md` (open item OF-08).
- Status: `assumed`
- Exercised by oracle cases (the tag stays `assumed`: a design choice of Caladrius, settled only with OM-08): the near-degenerate points (alpha/beta from 1 + 1e-3 down to k12 = 1e-12, with k10 = k21 and with k10 ≠ k21), the neighbours of ka = alpha and ka = beta and the weights at small k12, all against the 256-bit values. The loss of the textbook forms is shown by the testkit's double-precision textbook implementation, which reproduces only 72 of the 107 cases to 1e-8 (those whose exponents and ka are well separated; `the_naive_textbook_forms_reproduce_the_exact_values`). The engine's own devices are rewritings, recorded in MOD-2C-25.
- Sources: design choice (golden rule 6: no NaN from 0/0, no digits lost); MOD-AB1-03, MOD-NUM-01 (the one-compartment precedents); D; H12, H13 for the numbers.

#### MOD-2C-22 Overflow, underflow and what the engine refuses
Every exponent is negative, so e^(−λ·t) underflows to 0 and is correct as 0; the products t·e^(−a·t) are formed as in MOD-NUM-01. A concentration, area or derived quantity that is not finite (small vc with a large dose, tiny cl, a macro `a` + `b` that overflows) is an `Overflow` error as in MOD-NUM-02, never an infinity or a NaN. Because all terms of the combined form are non-negative, no clamp to 0 is needed except before the lag. The complete list of refusals is MOD-2C-05 together with MOD-GEN-04 (NaN or infinite times, non-finite parameters, unknown or missing parameter names). Times are accepted unsorted (MOD-GEN-02).
- Status: `confirmed by oracle`
- Oracle: error group `numeric` (`numeric_overflow_c0`: D/vc overflows; `numeric_overflow_derived_rate`: q/vp; `numeric_overflow_macro`: a + b; `numeric_degenerate_exponents`), group `times` (`times_nan`, `times_infinite`), and the underflow of the value cases (times up to 800 mean lives of beta, exact 0 expected; `extreme_slow` and `extreme_fast`).
- Sources: golden rule 6 of `AGENTS.md`; MOD-NUM-01, MOD-NUM-02 (precedent); D.

#### MOD-2C-23 Identifiability and ambiguity (for the fit)
(a) Labels: in the macro set α > β is a convention; the same curve arises with the pairs (A, α) and (B, β) exchanged, which is why the engine refuses α ≤ β instead of sorting (MOD-2C-05). (b) With first-order absorption the three exponents of the profile are exchangeable as shapes: a profile with exponents λ1 > λ2 > λ3 can be read with ka = λ1 (usual) or with ka = λ2 or λ3 (flip-flop cases, MOD-AB1-06); not every assignment gives positive micro-constants, and the number of admissible readings is not specified here (open item OM-09). The initial estimates assume ka > α > β by default (`specs/fit.md` FIT-INI-03). (c) As Q → 0 or Vp → 0, one phase disappears (wα → 0 or wβ → 0) and Vp, then Q, become unidentifiable (∂C/∂vp → 0); the fit reports a singular or ill-conditioned covariance matrix with the usual NC and a readable reason, and the bounds of FIT-BND-03 (lower bound 1e-6 × the initial estimate) keep the model inside MOD-2C-05. (d) From concentrations of an extravascular dose only cl/F, vc/F, q/F, vp/F are identifiable (MOD-GEN-03).
- Status: `assumed`
- Oracle: only (a) is exercised: `macro_alpha_below_beta` and `macro_alpha_equals_beta` (the engine refuses and does not sort). (b) to (d) concern the fit and have no oracle case (OM-09; the fit of a two-compartment model has no oracle fit, see `specs/fit.md` OF-08).
- Sources: S-17, S-18 (flip-flop, identifiability of polyexponential decompositions, book level); S-33; D; `specs/fit.md` FIT-INI-03, FIT-BND-03.

#### MOD-2C-24 Multiple dosing (later)
Superposition of single-dose profiles as MOD-MD-01. At steady state with equal doses at interval τ, each exponential e^(−λ·t) of the single-dose function (λ ∈ {α, β, ka}) is multiplied by 1/(1 − e^(−λ·τ)); in the combined form the two one-compartment steady-state functions are combined with the weights wα, wβ. Not specified further; a later card.
- Status: `documented, untested`
- Sources: S-17, S-18 (superposition, book level); S-20 (steady-state two-compartment functions exist in the catalogue); D.

#### MOD-2C-25 Numerical choices of the engine, and the oracle's rule for tiny derivatives
These are rewritings of the same functions, not method differences (nothing is added to `specs/differences.md`): the oracle's values are reproduced at 1e-12 with them.
- Kernel. The concentration, the AUC and the AUMC of every id are built from one unit-volume one-compartment kernel Φ(λ, s) and its integral and first moment, as MOD-2C-06 allows: wα·Φ(α) + wβ·Φ(β) over Vc. The first moment of the bolus, zero-order and first-order inputs is written without cancellation (three regimes for the first-order input, according to the relative size of ka and λ).
- Derivatives. The chain of MOD-2C-19 is evaluated with closed forms of the derivatives of α, β and w that are products and quotients of positive numbers (rather than differences of the exponents), and the macro set directly (MOD-2C-18). Three devices were needed to reach 1e-12 at condition numbers up to 909. (a) When (α − β)·s ≤ 1 the difference Φ(α, s) − Φ(β, s) is computed as the integral of ∂Φ/∂λ over [β, α] by an 8-point Gauss–Legendre rule; the integrand is an entire function of λ on a scale 1/s, so on that interval the quadrature error is far below double precision; outside the regime the plain difference loses at most a few bits and is used. (b) In the same regime α and β are written about their mean, e^(−λ·s) = e^(−c·s)·e^(−h·s) with c the mean and h the half difference, so that the rounding of the large argument c·s is common to all terms of the chain and cancels (without it errors of 1.0 to 1.3e-12 appeared at t = 200 in the near-degenerate cases). (c) The derivative with respect to ka, when ka is above the exponent, is written in a form with no subtraction of nearly equal large terms (a plain form lost the 1e-12 at t = 4000 when ka ≫ λ).
- Scalars and Tmax. The first-order Tmax by bisection on the sign of C' (MOD-2C-15); `a_oral` and `b_oral` only away from the equalities (MOD-2C-20); overflow of a scalar derived from valid parameters is an `Overflow` error (MOD-2C-22).
- The oracle's rule for derivatives far below the double range. The 256-bit central differences cannot resolve a derivative smaller than about 1e-52 relative to the concentration next to a dominant term (the case: `d_alpha` of the macro set at late times, which is only the α-term, between about 1e-52 and 1e-228, next to a β-dominated concentration). An entry below the 256-bit noise is therefore recomputed at 2048 bits (noise 2^-1900 relative): a value above that noise is written with its value (35 entries of 13 macro cases, for example `deriv_iv_bolus_base_macro` at t = 160, about −2.9e-67); a value below it is a structural zero and is written as an exact 0, and so is a true value below the range of normal doubles (about 2.2e-308; 7 entries): the engine returns exact 0 there as well. 42 entries were recomputed in all; 44 structural zeros remain written as exact 0. Counters are in each `options` file of the derivative cases (`n_recomputed_at_2048_bits`, `n_structural_zero_written_as_zero`).
- Status: `confirmed by oracle`
- Oracle: the 64 `deriv_*` cases with the 508 of 508 tests (the 13 macro cases `deriv_<id>_base_macro`, `deriv_<id>_distribution_macro`, `deriv_oral_1_ka_eq_alpha_macro` are those of the 2048-bit rule, corrected on 2026-10-09); `iv_bolus_near_degenerate_1e6`, `deriv_iv_bolus_near_degenerate_1e6`, `deriv_oral_1_near_degenerate_1e6`, `deriv_oral_1_ab_ratio_1e3` for the three devices; the property tests of `oracle_models_2c.rs` (continuity at ka = alpha, time origin, unsorted times). The switch point (α − β)·s = 1 was also checked for continuity of the derivative columns by the reviewer of T-033 (second differences between 1e-15 and 2e-13 relative).
- Sources: D; reviewer notes of T-033 and the thread of T-032/T-033 (`board/tasks/T-033.md`, `board/messages/2026-10-09-engine-oracle-pk2-derivative-zeros.md`); MOD-2C-21 for the one-compartment precedents.

### 11.6 Worked examples

Reproduced by the oracle's test layer (card T-032) at the digits printed here (10 or 12 decimals): N1 to N8, by the tests `worked_example_n1_iv_bolus` … `worked_example_n8_partial_derivatives` of `crates/caladrius-testkit/tests/pk2_consistency.rs`, against the 256-bit values of the cases (they pass). Computed by the reader in a throwaway script with 70-digit decimal arithmetic, outside the repository (checks H11 to H14), and targets for unit tests and for the oracle. Common case for N1 to N5, N7 and N8: D = 100, Vc = 10, CL = 2, Q = 4, Vp = 8, hence k10 = 0.2, k12 = 0.4, k21 = 0.5, S = 1.1, α = 1 and β = 0.1 exactly (αβ = 0.1 = k10·k21), wα = 5/9, wβ = 4/9, A = 50/9 = 5.5555555556, B = 40/9 = 4.4444444444. Values to the digits shown.

**N1. IV bolus (`pk2.iv_bolus`).** C0 = 10. C(1) = 6.0652743089, C(4) = 3.0809537540, C(12) = 1.3386750763, C(24) = 0.4031909037. t½α = 0.6931471806, t½β = 6.9314718056. AUC(0, ∞) = A/α + B/β = 5.5555555556 + 44.4444444444 = 50 = D/CL; AUC(0, 4) = 20.1062444046, AUC(0, 24) = 45.9680909647. AUMC(0, ∞) = A/α² + B/β² = 5.5555555556 + 444.4444444444 = 450 = D·Vss/CL². MRT = 9 = Vss/CL, Vss = 18 = Vc + Vp, Vz = CL/β = 20, V_extrap = D/B = 22.5 (order Vc 10 < Vss 18 < Vz 20 < V_extrap 22.5, MOD-2C-13). C(24) agrees with a fourth-order Runge-Kutta solution of the ODEs (0.4031909037182, H11). For NCA of data on this curve, the α-term is below 1e-6 of the β-term from t ≥ 15.7 (MOD-2C-16 b).

**N2. IV infusion, T = 2 (R0 = 50; `pk2.iv_infusion`).** C(1) = 3.8706144848; C(2) = 6.4300519226 = Cmax at Tmax = 2; C(5) = 3.1037489142; C(24) = 0.4463378912. AUC(0, 2) = 7.3160986930, AUC(0, 24) = 45.5366210942, AUC(0, ∞) = 50. MRT of the profile = 9 + T/2 = 10, AUMC(0, ∞) = 500. The same numbers describe `pk2.oral_0` with the apparent quantities.

**N3. First-order absorption, ka = 2 (`pk2.oral_1`).** C(1) = 6.1838339644, C(4) = 3.3342105358 (Runge-Kutta: 3.3342105358, H11), C(12) = 1.4091639967. Oral-profile coefficients (MOD-2C-09): A_o = 11.1111111111, B_o = 4.6783625731, coefficient of e^(−ka·t) = −15.7894736842. Tmax = 0.9501291546, Cmax = 6.1898890498; the bracket of MOD-2C-15 is [ln 2/1, ln 20/1.9] = [0.6931471806, 1.5767009876] and contains Tmax. AUC(0, 12) = 35.9089744488, AUC(0, 24) = 45.7558852258, AUC(0, ∞) = 50, MRT of the profile = 9 + 1/2 = 9.5. Other ka: ka = 1 (= α): C(1) = 4.6954190034, C(4) = 3.6267890476, C(12) = 1.4877580966, Tmax = 1.5350257231, Cmax = 5.0089388633; ka = 0.1 (= β): C(1) = 0.7336055048, C(4) = 1.5941519381, C(12) = 1.7922876905, Tmax = 8.6170616924, Cmax = 1.8785197816.

**N4. The limits ka = α and ka = β and their neighbours (t = 3, `pk2.oral_1`).** ka = α = 1: C(3) = 4.242283990397 (the explicit limit form of MOD-2C-10 and the stable form agree to all digits shown). ka = α + 1e-3, +1e-6, +1e-9: 4.242226751871, 4.242283933924, 4.242283990341. ka = β = 0.1: C(3) = 1.414320067276; ka = β + 1e-3, +1e-6, +1e-9: 1.426062473717, 1.414331830891, 1.414320079039. The textbook three-exponential form (MOD-2C-09) in double precision gives 4.242226751871 at +1e-3 (correct), 4.242283933897 at +1e-6 (relative error 6e-12) and 4.242283962510 at +1e-9 (relative error 6.6e-9, instead of 4.242283990341): the loss the stable form removes.

**N5. Lag and zero-order with lag (tlag = 0.5).** `pk2.oral_1_lag`, ka = 2: C(0.5) = 0 and C(3) = 4.4491793389 (= the lag-free C(2.5) of N3); Tmax = 0.9501291546 + 0.5 = 1.4501291546, Cmax unchanged; MRT of the profile 9 + 0.5 + 0.5 = 10 (MOD-2C-14). `pk2.oral_0_lag`, T = 2: C(3) = 5.2885410903 (= the infusion function of N2 at 2.5); Tmax = T + tlag = 2.5.

**N6. Nearly equal exponents (`pk2.iv_bolus`; k10 = k21 = 0.2, k12 = 1e-9, Vc = 10, D = 100).** The stable discriminant gives r = α − β = 2.828427126514e-5 (the textbook S² − 4·k10·k21 in double precision gives 2.828427045497e-5, relative error 2.9e-8; at k12 = 1e-12 the figures are 8.944271910005e-7 against 8.944110913856e-7, relative error 1.8e-5). Weights wα = 0.500017677670, wβ = 0.499982322330. C(1) = 8.187307523411 and C(10) = 1.353352832366, which are 10·e^(−0.2) and 10·e^(−2) to 12 digits: the model has become a one-compartment model with k = 0.2. A second case, k10 = 0.2, k21 = 0.5, k12 = 1e-9: α = 0.500000001667, β = 0.199999999333, wα = 5.556e-9, a very small but positive weight.

**N7. Conversions both ways.** Forward: (cl, vc, q, vp) = (2, 10, 4, 8) gives (k10, k12, k21) = (0.2, 0.4, 0.5), then α = 1, β = 0.1, (a, b) = (50/9, 40/9). Reverse from (a, b, alpha, beta) = (50/9, 40/9, 1, 0.1) and D = 100: vc = 100/(a + b) = 10; w = 5/9; k21 = 1·(4/9) + 0.1·(5/9) = 0.5; k10 = 0.1/0.5 = 0.2; k12 = (5/9)(4/9)(0.9)²/0.5 = 0.4; cl = 2, q = 4, vp = q/k21 = 8. Oral coefficients for ka = 2: A_o = ka·A/(ka − α) = 2·(50/9)/1 = 11.1111111111 and B_o = 2·(40/9)/1.9 = 4.6783625731.

**N8. Partial derivatives (IV bolus, t = 1, clearance set).** Sensitivities of the exponents at this point: ∂α/∂(k10, k12, k21) = (0.5555555556, 1.1111111111, 0.8888888889), ∂β/∂(k10, k12, k21) = (0.4444444444, −0.1111111111, 0.1111111111) (each pair adds to 1). C(1) = 6.0652743089 and ∂C/∂(cl, vc, q, vp) = (−0.5869035023, −0.3130661305, −0.3180294080, −0.0610860459), reproduced to ten digits by 70-digit central differences with step 1e-25 (H14).

### 11.7 Hand checks of section 11

- **H11.** A fourth-order Runge-Kutta solution of the three ODEs (step 2e-5 for the oral case, 1.2e-3 for the bolus, double precision) reproduces C(4) of N3 (3.334210535799908) and C(24) of N1 (0.4031909037182288) to 12 digits.
- **H12.** The explicit limit forms of MOD-2C-10 and the stable combination of MOD-2C-06 agree to 12 digits at t = 3 for ka = α and ka = β (N4); the three-exponential form of MOD-2C-09 agrees with the combination at ka = α + 1e-3.
- **H13.** 70-digit evaluation of the neighbours of N4 and of the Tmax bracket of N3 (bisection on C′ inside the bracket for ka = 2, 1, 0.1); the AUCs of N1 to N3 are 70-digit evaluations of the combination of MOD-2C-06, so they are not independent of it: the independent check of the areas (ODE-integrated) is left to the oracle (OM-06).
- **H14.** The chain rule of MOD-2C-19 reproduces central differences at 70 digits (N8). In 200 000 random parameter sets (rates between e^−6 and e^3, Vc between 0.1 and 50) the order Vc < Vss < Vz < V_extrap of MOD-2C-13 held in every one.

### 11.8 Open items (two compartments)

| id | item | rule | who and how |
|---|---|---|---|
| OM-06 | CLOSED 2026-10-09 (T-032, T-033): the two-compartment model oracle exists in `oracle/expected/models/pk2/` (107 value cases, 11 145 values, 256-bit evaluation with matrix-exponential and ODE cross-checks, all in the three parameter sets where it matters) and the engine passes it (508 of 508). The list that was asked for, kept for the record: Cases it must produce (card T-032): for each of the six ids, the base case of N1 in the three parameterisations (they must give the same values), a case with a clear distribution phase (k12 much larger than k10) and one with a small peripheral exchange; closed-form grids with C(t), AUC(0, t), AUMC(0, t) (finite and to infinity), MRT, Vss, Vz, V_extrap, α, β, A, B, wα, the half-lives, `tmax_pred`, `cmax_pred`; an independent 256-bit evaluation of the explicit sums of this section (the textbook three-exponential forms, not the engine's combination), rounded once to double as in T-009, cross-checked within 1e-8 by a matrix exponential of the 3 × 3 (or 2 × 2) system and by an adaptive ODE solution (`deSolve`) that also integrates the AUC and the first moment; tolerance 1e-12 relative, an expected zero exact; grids with t = −1, t = 0, the lag and duration boundaries | all | oracle card T-032 |
| OM-07 | CLOSED 2026-10-09 (T-032, T-033): all the cases below exist (value cases of MOD-2C-10 and 21, 80 refusals of MOD-2C-05, 22 and MOD-GEN-04) and pass. Oracle edge cases: ka = α, ka = β, and ka = α ± 1e-3, 1e-6, 1e-9 and the same for β; ka between α and β; ka below β (flip-flop shape); ka/α = 500; k12 = 1e-3, 1e-6, 1e-9, 1e-12 with k10 = k21 and with k10 ≠ k21; α/β = 1e3; very small and very large times (underflow); dose 0; the error cases of MOD-2C-05 (q = 0, vp = 0, alpha ≤ beta, macro without dose, mixed sets, unknown names) as specification tests | MOD-2C-05, 10, 21, 22 | oracle card T-032 |
| OM-08 | Which parameter sets and names the reference software uses for two compartments, whether its oral macro models use the IV coefficients or the oral-profile coefficients (MOD-2C-02, MOD-2C-09), whether it sorts α and β | MOD-2C-02, 05, 20 | question Q-009 (names); the headers of the human's exports |
| OM-09 | The number of admissible readings of a three-exponential oral profile (flip-flop cases) and the warning a fit must give | MOD-2C-23 | later card; `specs/fit.md` OF-08, OF-11 |
| OM-10 | CLOSED 2026-10-09 (T-032, T-033): 64 derivative cases, 5 681 values, 256-bit central differences, engine analytic Jacobian at 1e-12; the 393 ill-conditioned entries are omitted, see OM-14. Derivative oracle: gradients of C in the three parameterisations against 256-bit central differences (or complex-step evaluation), at the base case and near α ≈ β; the partial-derivative table of the fit against the analytic Jacobian | MOD-2C-17 to 19 | oracle card T-032, engine card |
| OM-11 | Textbook worked examples with page references: none can be cited yet, because the reader cannot read the books. Candidates for someone holding them: the two-compartment IV bolus and oral examples of Gibaldi and Perrier (S-18, the chapters on the two-compartment open model and on multicompartment absorption); the worked examples of the compartmental chapters of Gabrielsson and Weiner (S-17); the numerical examples of Wagner 1976 (S-33), which relate coefficients and exponents to CL, Vss and Vβ. Each must give chapter, example and page and the printed digits, and be recomputed by the engine within the printed precision | all | someone holding the books (as OM-05) |
| OM-12 | Steady state and multiple dosing for two compartments, three compartments, Michaelis-Menten elimination | MOD-2C-24 | later cards |
| OM-13 | Section and page numbers for the two-compartment equations in S-17 and S-18 (MOD-2C-01 to 16); the pages of S-34 were taken from reference lists and not checked at the publisher | all | someone holding the books |
| OM-14 | The 393 derivative entries omitted by the oracle because their condition number exceeds 1e3 (zero crossings and near-degenerate points, MOD-2C-19) are returned by the engine but checked by no test. A later oracle card could add them with a conditioning-aware method (for example a higher working precision for the differences, or complex-step evaluation, with a tolerance relative to the sum of the absolute terms); it must not loosen `MODEL_DERIVATIVES` | MOD-2C-19, 25 | later oracle card |
| OM-15 | NCA run on pk2-simulated profiles compared with the model's AUC, AUMC, MRT, Vss and terminal half-life (oracle case for the oracle agent) | MOD-2C-16 | oracle agent |