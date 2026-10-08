# One-compartment models: behaviour specification

Card T-008. Written by the `reader` agent on 2026-10-08, in its own words and formulas. Sources are cited by the keys of `specs/sources.md` (S-01 …; the keys added for this file are in its section 7); `D` is a derivation shown here, `H` a hand or scripted check by the reader (section 9).

## 0. How to read this file

**Status tags** (see `specs/README.md`): `confirmed by oracle`, `documented, untested`, `assumed`. No oracle for models exists yet (step 3 of the marching order has not started), so nothing here is `confirmed by oracle`. A closed-form expression that follows from the differential equations by calculus, and that the reader also checked against a numerical integration of those equations (H1), is `documented, untested`; a choice made for Caladrius (names, validation, numerical safeguards) is `assumed`.

**Rule ids.** `MOD-<AREA>-<nn>`. Areas: GEN (conventions and domains), IVB (intravenous bolus), IVI (intravenous infusion), AB1 (first-order absorption), AB0 (zero-order absorption), SEC (secondary parameters), NUM (numerics), MD (multiple dosing, later), VOC (vocabulary).

**Clean room.** The reference software's own documentation was not used (`specs/sources.md` section 2). The textbooks cannot be read by the reader (S-17, S-18 are cited at book level only). The equations below are therefore derived (D) from the linear differential equations of the compartment diagram, which is how the textbooks obtain them, and compared with the model catalogue and the CL/V convention of two open packages (S-20, S-21). Where a rule is a design choice it says so.

**Scope (AGENTS.md section 1).** One compartment, linear first-order elimination, single dose, closed form. Two compartments, user-written ODE models and several subjects are later steps. Multiple dosing is noted in MOD-MD-01 and not specified further.

**Numerics.** Double precision. Nothing is rounded inside the computation. A function value that is not finite is an error of the caller's parameters (MOD-GEN-04), never a silent NaN.

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
- Status: `documented, untested`
- Sources: S-20 (the same catalogue of six single-dose one-compartment models); S-21 (the manual writes the oral theophylline model as a depot and a central compartment, concentration = central amount over volume); S-17, S-18 (textbook chapters on the one-compartment model, book level only); D.

### MOD-GEN-02 Time origin and dose time
All formulas take t = time since the dose. A time before the dose (t < 0) gives C = 0, also for the IV bolus (the drug is not there yet). A point exactly at t = 0 is evaluated by the formula (IV bolus: D/V; extravascular: 0). Lag times add to the dose time: the effective time is t − tlag.
- Status: `assumed`
- Sources: design choice, consistent with `specs/nca.md` NCA-DAT-01 (samples before the dose are not used).

### MOD-GEN-03 Parameterisation and bioavailability
The primary parameters are V and CL; k = CL/V is derived. V and k are an equivalent pair (CL = V·k). The user chooses one pair; results show both. For extravascular input the bioavailability F multiplies the dose: only F·D is observable from concentrations, so V and CL are the apparent V/F and CL/F, and the model takes the effective dose D_eff = F·D with F fixed (default 1) and never estimated together with V and CL. The unit of V/F is [D]/[C], the unit of CL/F is [D]/([T]·[C]) (`specs/nca.md` NCA-UNIT-01).
- Status: `documented, untested`
- Sources: S-20 (parameters CL and V, dose, ka, tlag, duration); S-21; S-01:pk.calc.cl and pk.calc.vz in the `specs/nca.md` sense (apparent quantities); D (F and V enter the equations only as F·D/V).

### MOD-GEN-04 Parameter domains and errors
V > 0, CL > 0 (so k > 0), ka > 0, T > 0, tlag ≥ 0, D > 0 are required, all finite. A parameter outside its domain is a readable error naming the parameter and the value, raised before any evaluation. A dose of 0 is allowed and gives C = 0 everywhere (useful for a placebo or a washout record). A time that is NaN or infinite is an error. A time of +∞ is not evaluated.
- Status: `assumed`
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

The prefix `pk1` means one compartment; `pk2` is reserved for the next step. The six ids coincide one to one with the six single-dose functions of S-20.
- Status: `assumed`
- Sources: S-20 (catalogue); `AGENTS.md` sections 4 and 7.

## 3. Intravenous bolus

### MOD-IVB-01 Concentration
C(t) = (D/V)·e^(−k·t) for t ≥ 0. The initial concentration is C0 = D/V. With CL and V: C(t) = (D/V)·exp(−(CL/V)·t).
- Status: `documented, untested`
- Sources: S-17, S-18 (book level); S-20; D (solution of dA/dt = −kA, A(0) = D). Checked in worked example M1.

### MOD-IVB-02 Derived quantities
Half-life t½ = ln 2/k. AUC from 0 to infinity = D/CL = C0/k; AUC(0, t) = (D/CL)·(1 − e^(−k·t)). AUMC to infinity = D/(V·k²) = C0/k². MRT = 1/k. Vss = V. Tmax and Cmax: not defined (the maximum is C0 at t = 0).
- Status: `documented, untested`
- Sources: S-17, S-18; `specs/nca.md` NCA-EXT-04 to EXT-07 and W2 (the same relations in NCA, MRT = 5 and Vss = 10 for k = 0.2, V = 10); D (integrals of e^(−kt) and t·e^(−kt)).

## 4. Intravenous infusion

### MOD-IVI-01 Concentration during and after the infusion
Let R0 = D/T. For 0 ≤ t ≤ T: C(t) = (R0/CL)·(1 − e^(−k·t)). For t > T: C(t) = C(T)·e^(−k·(t − T)) with C(T) = (R0/CL)·(1 − e^(−k·T)). Equivalently, for t > T, C(t) = (R0/CL)·(1 − e^(−k·T))·e^(−k·(t − T)). The two branches agree at t = T, and the infusion reduces to the bolus when T → 0 with D fixed.
- Status: `documented, untested`
- Sources: S-17, S-18; S-20 (the infusion function takes dose, CL, V and the infusion duration); D (solution of dA/dt = R0 − kA for t ≤ T, then free decay). Checked in worked example M3 and by numerical integration (H1).

### MOD-IVI-02 Derived quantities
Cmax = C(T) at Tmax = T (the concentration rises during the infusion and falls after it). AUC to infinity = D/CL. AUC(0, T) = (R0/CL)·(T − (1 − e^(−k·T))/k) and AUC(T, ∞) = C(T)/k, which add up to D/CL. MRT (the IV form of `specs/nca.md` NCA-EXT-04) = 1/k + T/2 for the profile moment ratio AUMC/AUC; the disposition MRT of the system is 1/k. Vss = V.
- Status: `documented, untested`
- Sources: `specs/nca.md` NCA-EXT-04, EXT-07 and W5 (AUMC/AUC = 1/k + T_inf/2); S-01:pk.calc.mrt; D.

## 5. First-order absorption

### MOD-AB1-01 Concentration without lag
For ka ≠ k and t ≥ 0: C(t) = D·ka/(V·(ka − k)) · (e^(−k·t) − e^(−ka·t)). With D the effective dose F·D and V, CL the apparent quantities (MOD-GEN-03). C(0) = 0.
- Status: `documented, untested`
- Sources: S-17, S-18 (the Bateman function, book level); S-20; D (solution of the depot–central pair of equations). Checked in worked example M2 and by numerical integration (H1).

### MOD-AB1-02 The limit ka = k
For ka = k: C(t) = D·k·t·e^(−k·t)/V. This is the continuous limit of MOD-AB1-01 as ka → k (D: the difference quotient (e^(−kt) − e^(−ka·t))/(ka − k) tends to t·e^(−kt)).
- Status: `documented, untested`
- Sources: D; worked example M5.

### MOD-AB1-03 Evaluation near ka = k
Evaluating MOD-AB1-01 as written loses digits when |ka − k| is small relative to k. The implementation evaluates the factor (e^(−k·t) − e^(−ka·t))/(ka − k) as t·e^(−k·t)·g(x) with x = (ka − k)·t and g(x) = (1 − e^(−x))/x computed with the exact `expm1` form, g(0) = 1, so that no digits are lost to cancellation for any ka > 0 and k > 0, and the case ka = k needs no branch of its own beyond g(0) = 1. Tests: for |x| ≤ 1e-3 the value of g agrees with the series 1 − x/2 + x²/6 − x³/24 to a relative 1e-12; for ka/k = 2 and 10 the result agrees with the textbook form of MOD-AB1-01 to 1e-12 relative; for ka = k it equals MOD-AB1-02.
- Status: `assumed`
- Sources: design choice (golden rule 6: no NaN from 0/0); D (e^(−kt) − e^(−ka·t) = e^(−kt)·(1 − e^(−x))); worked example M5 shows the effect.

### MOD-AB1-04 Tmax, Cmax and AUC
For ka ≠ k: Tmax = ln(ka/k)/(ka − k); Cmax = (D/V)·e^(−k·Tmax) = (D/V)·(k/ka)^(k/(ka − k)). For ka = k: Tmax = 1/k and Cmax = D/(V·e). AUC from 0 to infinity = D/CL = D/(V·k) (independent of ka). AUC(0, t) = D/(V·(ka − k)) · ( (ka/k)·(1 − e^(−k·t)) − (1 − e^(−ka·t)) ). With a lag, Tmax is increased by tlag and Cmax is unchanged.
- Status: `documented, untested`
- Sources: S-17, S-18 (book level); D (set dC/dt = 0, use ka·e^(−ka·Tmax) = k·e^(−k·Tmax); integrate the two exponentials); worked example M2 (AUC to infinity = 50 for D = 100, CL = 2).

### MOD-AB1-05 Lag time
With a lag, C(t) = 0 for t ≤ tlag and C(t) = the lag-free function of MOD-AB1-01/02 evaluated at t − tlag for t > tlag. The function is continuous at tlag (it is 0 on both sides) but has a kink there.
- Status: `documented, untested`
- Sources: S-20 (separate lag variants of the first-order and zero-order models); S-21; D.

### MOD-AB1-06 Flip-flop
The concentration function is unchanged when the roles of ka and k are exchanged and V is replaced by V·k/ka (D: after the exchange the prefactor is D·k/(V'·(k − ka)) and the bracket is e^(−ka·t) − e^(−k·t), whose sign change cancels that of (k − ka); the two functions are equal exactly when k/V' = ka/V). Data alone then cannot tell "fast absorption and slow elimination" from "slow absorption and fast elimination". The model definition requires nothing; fitting and initial estimates assume ka > k by default (`specs/fit.md` FIT-INI-03) and warn that the other solution exists. Worked example M4 gives the pair.
- Status: `documented, untested`
- Sources: S-17, S-18 (flip-flop kinetics, book level); D; H2.

## 6. Zero-order absorption

### MOD-AB0-01 Concentration without lag
A constant input at rate R0 = D/T for 0 ≤ t ≤ T into the central compartment with first-order elimination. The concentration is the one of MOD-IVI-01 with D the effective dose F·D: for 0 ≤ t ≤ T, C(t) = (R0/CL)·(1 − e^(−k·t)); for t > T, C(t) = C(T)·e^(−k·(t − T)). The only difference from an IV infusion is the meaning of the parameters (V/F, CL/F apparent, T the absorption duration).
- Status: `documented, untested`
- Sources: S-20 (zero-order oral function with parameter `dur`, the absorption duration, same V and CL); D.

### MOD-AB0-02 With lag
Shift as in MOD-AB1-05: C(t) = 0 for t ≤ tlag; for t > tlag use MOD-AB0-01 at t − tlag. Tmax = tlag + T and Cmax = C(tlag + T).
- Status: `documented, untested`
- Sources: S-20; D; worked example M3 (lag variant).

### MOD-AB0-03 Rate or duration
The user may state the input as a duration T (default) or as a rate R0 with T = D/R0. Only one of the two is a parameter; the other is derived. Fitting uses the duration (a positive quantity with a natural bound).
- Status: `assumed`
- Sources: design choice; S-20 (duration is the parameter of the catalogued functions).

## 7. Secondary parameters

### MOD-SEC-01 Half-life
t½ = ln 2/k.
- Status: `documented, untested`
- Sources: S-17, S-18; `specs/nca.md` NCA-LZ-10 (same formula for λz); D.

### MOD-SEC-02 List per model
Reported when the model has them (value, unit, and a standard error in the fit, `specs/fit.md` FIT-OUT-06): k, t½, CL, V, AUC to infinity (= D/CL), Cmax and Tmax (analytic where they exist: oral first-order and zero-order, infusion), C0 (IV bolus), MRT (system, 1/k), MRT of the profile (1/k + 1/ka, plus tlag, for first-order oral; 1/k + T/2, plus tlag, for zero-order input), Vss (= V). The mean absorption time is 1/ka for first-order input and T/2 for zero-order input.
- Status: `documented, untested`
- Sources: `specs/nca.md` NCA-EXT-04 (MRT with the infusion half-duration); S-17, S-18; D (statistical moments: each first-order stage adds its mean transit time). Worked example M2 (MRT = 6 for ka = 1, k = 0.2).

### MOD-SEC-03 Consistency with NCA
For data lying exactly on a model, the NCA of `specs/nca.md` and the analytic values agree when the sampling is dense and extends far enough: AUC to infinity, λz = k, half-life, Cmax and Tmax of the grid, CL, Vz and Vss (IV). The checks are test properties (independent computation, AGENTS.md section 5), with the tolerance of the exponential-trapezoid error of NCA-AUC-10 for sparse grids.
- Status: `documented, untested`
- Sources: `specs/nca.md` W2 (the exact mono-exponential, same numbers as example M1 here: AUC to 12 h = 45.4641023355, AUC to infinity = 50); D.

## 8. Numerics, later work, vocabulary

### MOD-NUM-01 Overflow and underflow
e^(−k·t) with k·t large underflows to 0 and is correct as 0. An argument that would overflow cannot occur for valid domains (the exponents are non-positive). The function returns 0 for t before the lag, never −0.0 or a tiny negative value from cancellation: for the first-order model, a result below 0 by rounding (possible only for t very close to tlag) is clamped to 0.
- Status: `assumed`
- Sources: design choice (golden rule 6).

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
| `rate` | zero-order input rate (derived) | R0 | [D]/[T] |
| `f` | bioavailability (fixed, never fitted with V and CL) | F, Favail | none |
| `dose` | dose | dose, DOSE | [D] |
| `half_life` | ln 2/k | t½ | [T] |
| `c0` | D/V, IV bolus | C0 | [C] |
| `cmax_pred`, `tmax_pred` | analytic peak concentration and time | | [C], [T] |
| `auc_inf` | D/CL | | [T]·[C] |
| `mrt` | system or profile mean residence time (MOD-SEC-02) | | [T] |
| `vss` | V for one compartment | | [D]/[C] |

The suffix `_pred` separates the model-based Cmax and Tmax from the observed `cmax` and `tmax` of `specs/nca.md` section 10.
- Status: `assumed`
- Sources: S-20, S-21 (aliases); `specs/nca.md` section 10 (naming style); `AGENTS.md` section 7.

## 9. Worked examples and checks

Numbers were computed by the reader in a throwaway script kept outside the repository (hand checks H, to the digits shown) and are targets for unit tests. In every example D = 100, V = 10, CL = 2, k = 0.2.

**H1. Closed forms against numerical integration.** The reader integrated the differential equations of each model with a fourth-order Runge–Kutta scheme (step 1e-4) and compared with the closed forms at the times used below: agreement to better than 1e-12 relative for the oral model with lag (value 6.2378070966 at 3 h after the start of absorption, ka = 1) and the infusion model (8.2419988491 at the end of the 2 h infusion; 4.5233048731 at 5 h), and the trapezoidal sum of the oral concentration function over 0 to 6 h (600 000 intervals) reproduces the closed-form AUC(0, 6) = 31.2063461577 to 1e-10.

**M1. IV bolus (`pk1.iv_bolus`).** C0 = 10; C(1) = 10·e^(−0.2) = 8.1873075308; C(4) = 10·e^(−0.8) = 4.4932896412; C(12) = 0.9071795329. t½ = ln 2/0.2 = 3.4657359028. AUC to infinity = 100/2 = 50; AUC(0, 4) = 50·(1 − e^(−0.8)) = 27.5335517941; AUC(0, 12) = 50·(1 − e^(−2.4)) = 45.4641023355 (the same number as the NCA worked example W2). AUMC to infinity = 100/(10·0.04) = 250 and MRT = 5.

**M2. First-order absorption, ka = 1 (`pk1.oral_1`).** The prefactor is D·ka/(V·(ka − k)) = 100/(10·0.8) = 12.5, so C(t) = 12.5·(e^(−0.2t) − e^(−t)). C(0.5) = 12.5·(0.9048374180 − 0.6065306597) = 3.7288344790; C(1) = 5.6356413988; C(2) = 12.5·(0.6703200460 − 0.1353352832) = 6.6873095350; C(6) = 3.7339432467; C(12) = 1.1338976135. Tmax = ln 5/0.8 = 2.0117973905; Cmax = 10·e^(−0.2·2.0117973905) = 6.6874030498 (also 10·(0.2)^(0.25) = 6.6874030498: here k/(ka − k) = 0.25). AUC(0, 6) = 100/(10·0.8)·( 5·(1 − e^(−1.2)) − (1 − e^(−6)) ) = 31.2063461577; AUC to infinity = 50. MRT of the profile = 1/0.2 + 1/1 = 6. With tlag = 0.5, C(1.5) = C(1) of the lag-free model = 5.6356413988, C(0.5) = 0 and Tmax = 2.5117973905 with the same Cmax.

**M3. Infusion and zero-order absorption with lag (`pk1.iv_infusion`, `pk1.oral_0_lag`).** T = 2, R0 = 50, R0/CL = 25. C(1) = 25·(1 − e^(−0.2)) = 4.5317311731. C(2) = 25·(1 − e^(−0.4)) = 8.2419988491 = Cmax at Tmax = 2. C(5) = 8.2419988491·e^(−0.6) = 4.5233048731. AUC(0, 2) = 25·(2 − 0.3296799540/0.2) = 8.7900057545; AUC(2, ∞) = 8.2419988491/0.2 = 41.2099942455; the sum is 50. MRT of the profile = 1/0.2 + 2/2 = 6 (the NCA identity W5 with AUMC/AUC = 6). The same numbers describe `pk1.oral_0` with the apparent V/F and CL/F. With tlag = 0.5 (`pk1.oral_0_lag`): C(3) = C(2.5 without lag) = 8.2419988491·e^(−0.1) = 7.4576689581, and Tmax = 2.5.

**M4. Flip-flop pair.** (ka = 1, k = 0.2, V = 10) and (ka' = 0.2, k' = 1, V' = V·k/ka = 2, i.e. the roles exchanged) give the same concentrations: C(1) = 5.6356413988 and C(3) = 6.2378070966 for both. In the second set the declining phase is governed by the "absorption" constant; the NCA terminal slope 0.2 is then ka', not the elimination rate constant 1.

**M5. The case ka = k and its neighbours.** For ka = k = 0.2: C(5) = 100·0.2·5·e^(−1)/10 = 3.6787944117. For ka = 0.2 + 1e-3, 1e-6, 1e-9: C(5) = 3.6879607985, 3.6788036087, 3.6787944006: the value tends to the limit linearly in ka − k. A direct evaluation of the textbook form divides a difference of two nearly equal numbers (about 1e-9 relative to their size) by 1e-9, so it keeps only about seven of the sixteen digits in the last case; the form of MOD-AB1-03 keeps them all.

## 10. Open items

| id | item | rule | who and how |
|---|---|---|---|
| OM-01 | No oracle for model profiles exists. A public oracle needs an independent computation (testkit, second implementation) and, for the fits, a reference such as a published fit; the closed forms are testable against a numerical ODE integrator in `caladrius-testkit` without any reference software | all | oracle agent, step 3 |
| OM-02 | Which models and which parameter names the reference software offers are not known to the reader (documentation is off limits); the reader's catalogue follows two open packages (S-20, S-21). Compare names with the headers of the human's exports | MOD-GEN-05, MOD-VOC-01 | question Q-009 |
| OM-03 | Whether the reference software's zero-order absorption has its own "lag" or treats lag separately, and whether its first-order model with ka = k is accepted | MOD-AB1-02, MOD-AB0-02 | private oracle / human's screen |
| OM-04 | Two-compartment models, Michaelis–Menten elimination, multiple dosing and steady state are not specified | MOD-MD-01 | later cards |
| OM-05 | Textbook section and page numbers for the one-compartment equations | all | someone holding the books (S-17, S-18) |
