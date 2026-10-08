# Sources consulted and their terms of use

Maintained by the `reader` agent. First written 2026-10-08 for card T-002. Every rule in `specs/nca.md` cites sources by the keys of section 3.

Naming note: `AGENTS.md` keeps the reference software's vendor and product names out of the docs. Section 2 has to record the address of the documentation site it checked, so the vendor host name appears there and nowhere else in `specs/`. Question Q-006 in `board/QUESTIONS.md` asks the human to confirm this is acceptable.

## 1. Result in one paragraph

The terms attached to the reference software's public documentation do not authorise the use this project would make of it (an automated agent reading it to write an independent specification for an open-source tool). They tie the documentation to the vendor's customers and to a licence agreement the reader cannot see, and they forbid reproduction. The reader therefore treated the use as forbidden, did **not** use the reference documentation as a source of any rule, and recorded the matter as question Q-005. The specification rests on the PKNCA documentation, peer-reviewed papers, and the public documentation of other NCA tools. Where the reference software's conventions cannot be established from those, the rule is tagged `assumed` and is a test task for the oracle (observed results), not a documented fact.

## 2. Reference software documentation: terms-of-use check

Date of the check: 2026-10-08.

### 2.1 What was opened

Three pages, all through a page-reading tool that returns a machine summary rather than the page verbatim (a fourth address, a guessed terms-of-use path on the vendor website, answered "not found" and returned no content):

1. A documentation topic page (about the λz settings) on the documentation host `onlinehelp.certara.com`, opened **only** to find the link to its legal notice. The prompt asked for legal text and footer links and explicitly not for technical content. The tool returned only footer information (a rights-reserved line and the link to the legal notice). No technical content was taken from it.
2. The legal notice shipped with that documentation: `https://onlinehelp.certara.com/phoenix/8.5/Phoenix_UserDocs/LegalNotice/LegalNotice.htm` (copyright range 2005-2024).
3. The vendor website's legal notice: `https://certara.com/legal` (last-updated line 5 September 2025).

### 2.2 What the terms say (paraphrase of the tool's summary, which also gave a few short quotes)

Documentation legal notice:

- The software and its documentation may be used only as the controlling licence agreement authorises.
- The documentation is for use by the vendor's customers (and those of its affiliates and designates) only; use for any other purpose is not authorised.
- No part of the software or documentation may be reproduced, transmitted or translated, in any form or by any means, unless the licence agreement allows it or the vendor gives prior written permission.
- Silent on derivative works, reverse engineering and building other software. It refers to the licence agreement without linking it.

Website legal notice (it says it covers "this website"; it does not say it covers the documentation host):

- Site material is the vendor's property. Users may not modify, reproduce, upload, post, transmit, download or distribute it.
- Printing or downloading portions is allowed only for personal, informational, non-commercial use, with notices intact; any other use of the vendor's materials is prohibited.
- No robot, spider or other automatic device or process may be used to access the site for any purpose, and manual monitoring or copying of site material needs prior written consent.
- Silent on product documentation and on competing products.

Limit of this record: the reader saw summaries and short quotes, not the full legal texts. If the human decides to authorise the use, he should read the two notices in full.

### 2.3 Assessment

- The documentation is offered to customers under a licence agreement. The agreement that applies here is the human's university licence, which the reader cannot see (question Q-002 is still pending). Nothing in the notices gives a third party, or an automated agent working for a public open-source project, permission to read the documentation for this purpose.
- Rewriting facts and formulas in our own words is not "reproducing" the documentation in the usual sense, but the notices do not say it is allowed, and the "customers only, other purposes not authorised" clause points the other way.
- The main website notice forbids automated access. Whether it reaches the documentation host is not stated, but the reader's tools are automated.
- The licence of the software itself is not available to the reader.

Result: **use not allowed as the terms read; treated as forbidden.** Per `AGENTS.md` section 6 the reader stopped, did not read the reference documentation, and wrote question Q-005.

### 2.4 Incidental exposure, disclosed

Three web searches run on 2026-10-08 (two to locate the documentation site, one about how another NCA tool's λz selection compares with the reference software) returned tool-written summaries that restated parts of the reference documentation and of community forum threads about it. The reader did not ask for these pages and did not open any of them. The topics that appeared:

- the λz best-fit definition (largest adjusted R², a 1e-4 tie allowance in favour of more points, candidate sets built from the last 3, 4, 5 … points, zero concentrations skipped);
- which early points are left out of the automatic selection (before the maximum, during an infusion, and the maximum itself for non-bolus data);
- a setting for a minimum adjusted R² that only flags profiles;
- the existence of three or four AUC calculation options, their menu labels, and that a newer release may have changed the λz selection procedure (an unverified community report).

Handling: none of this is used as a source. Every rule in `specs/nca.md` on these topics is sourced from the register below, independently of those summaries. Anything the independent sources do not settle is tagged `assumed`. The orchestrator can judge whether this exposure matters for the clean-room record.

Forum threads whose titles appeared in search results were not opened. Later searches excluded the vendor's domains.

### 2.5 Where the reference software's behaviour was learned instead

- A public conference poster comparing three NCA programs, one being the reference software (S-04).
- A peer-reviewed open-access paper on an NCA package built to reproduce the reference software, with a published comparison table (S-05).
- The manuals of two R packages written to reproduce the reference software (S-06, S-07).
- The public documentation of a commercial PK tool that uses the same parameter names (S-08, S-09).

None of these is the reference software's own documentation.

## 3. Register of sources used

Terms column: what the source's licence or terms say about our use, as far as the reader checked.

| key | source | location | terms and how used |
|---|---|---|---|
| S-01 | PKNCA 0.12.1 reference manual (CRAN, 2025-08-19). Topics cited as `S-01:<topic>` (e.g. `pk.calc.auxc`, `pk.calc.half.life`, `pk.calc.c0`, `clean.conc.blq`, `assert_conc_time`). | `https://cran.r-project.org/web/packages/PKNCA/refman/PKNCA.html` | AGPL-3 (package licence). Read only. Facts, formulas and defaults re-expressed; no sentence copied. The package source code was not read. |
| S-02 | PKNCA vignettes: v01 Introduction and usage; v05 AUC calculation; v06 Half-life calculation; v07 Unit assignment and conversion; v08 Data imputation; v23 AUC integration methods; v31 Introduction article; v40 Options. Cited as `S-02:v05` etc. | `https://cran.r-project.org/web/packages/PKNCA/vignettes/` (same files on `https://humanpred.github.io/pknca/articles/`) | AGPL-3. As S-01. A few printed example results are quoted as numbers in `nca.md` (worked example W1, λz targets W8); row added to `ATTRIBUTION.md`. |
| S-03 | PKNCA NEWS file, versions 0.6 to 0.12.1 (behaviour changes: edge cases, defaults, bug fixes). | `https://cran.r-project.org/web/packages/PKNCA/news/news.html` | AGPL-3. As S-01. |
| S-04 | Conference poster T-102, ACOP 2019: comparison of NCA results between PKNCA, one other open tool and the reference software, on simulated profiles (Ma, Guglieri-Lopez, Gobburu, Denney, Ivaturi). | `http://www.humanpredictions.com/wp-content/uploads/2020/01/ACOP_2019_T102_NCA_performance_evaluation_Yingbo_revised.pdf` (linked from the PKNCA manual, S-01 topic `PKNCA`) | Public PDF, no licence stated. Cited and paraphrased only. The fetch tool cached the PDF outside the repository. |
| S-05 | Kim H, Han S, Cho Y-S, Yoon S-K, Bae K-S. Development of R packages: NonCompart and ncar for noncompartmental analysis (NCA). Transl Clin Pharmacol 2018;26(1):10-15. doi:10.12793/tcp.2018.26.1.10 (PMC6989226). Its Table 1 maps output names, its Table 3 compares package output with the reference software on a public dataset. | `https://pmc.ncbi.nlm.nih.gov/articles/PMC6989226/` | CC BY-NC 3.0. The non-commercial clause does not fit an MIT OR Apache-2.0 repository, so no text or table is copied. Used to check rules and to establish names. See question Q-007 about the published numbers. |
| S-06 | NonCompart 0.8.4 reference manual (CRAN, 2026-09-21): `sNCA`, `BestSlope`, `LinAUC`, `LogAUC`, `AUC`. | `https://cran.r-project.org/web/packages/NonCompart/refman/NonCompart.html` | GPL-3. Facts only. Outside the card's explicit list; used only as a second source. |
| S-07 | pkr 0.1.6 reference manual (CRAN, 2026-07-30): `BestSlope`, `IndiNCA`, `AUC`. | `https://cran.r-project.org/web/packages/pkr/refman/pkr.html` | GPL-3. Facts only. Outside the card's explicit list; second source. |
| S-08 | PKanalix "Parameters" page (NCA parameter names, CDISC codes, formulas). | `https://pkanalix.lixoft.com/nca-parameters/` | Simulations Plus documentation. The terms page of the company website (read 2026-10-08) covers only its main website, forbids reproduction there, and says nothing on the documentation host. Reading and facts only; nothing copied. Outside the card's explicit list; second source for names and formulas. |
| S-09 | PKanalix "Data processing and calculation rules" page (BLQ options, λz candidate rules, AUC rules, C0 rules). Read through a page summary because the page is script-rendered. | `https://monolixsuite.slp-software.com/pkanalix/2024R1/data-processing-and-calculation-rules` | As S-08. The summary omitted formulas and defaults. Second source only. |
| S-10 | Purves RD. Optimum numerical integration methods for estimation of AUC and AUMC. J Pharmacokinet Biopharm 1992;20(3):211-226. doi:10.1007/BF01062525 | PubMed 1522479 | Bibliographic record checked in PubMed (abstract read). Paper itself not read. |
| S-11 | Yeh KC, Kwan KC. A comparison of numerical integrating algorithms by trapezoidal, Lagrange, and spline approximation. J Pharmacokinet Biopharm 1978;6(1):79-98. doi:10.1007/BF01066064 | PubMed 650423 | Record checked; paper not read. Background only, cited by no rule. |
| S-12 | Chiou WL. Critical evaluation of the potential error in pharmacokinetic studies of using the linear trapezoidal rule method for the calculation of the area under the plasma level-time curve. J Pharmacokinet Biopharm 1978;6(6):539-546. doi:10.1007/BF01062108 | PubMed 731416 | Record and abstract read in PubMed; paper not read. |
| S-13 | Yamaoka K, Nakagawa T, Uno T. Statistical moments in pharmacokinetics. J Pharmacokinet Biopharm 1978;6(6):547-558. doi:10.1007/BF01062109 | PubMed 731417 | Record and abstract read; paper not read. |
| S-14 | Benet LZ, Galeazzi RL. Noncompartmental determination of the steady-state volume of distribution. J Pharm Sci 1979;68(8):1071-1074. doi:10.1002/jps.2600680845 | PubMed 480170 | Record checked; paper not read. |
| S-15 | Perrier D, Mayersohn M. Noncompartmental determination of the steady-state volume of distribution for any mode of administration. J Pharm Sci 1982;71(3):372-373. doi:10.1002/jps.2600710332 | PubMed 7069605 | Record checked; paper not read. |
| S-16 | Gabrielsson J, Weiner D. Non-compartmental analysis. Methods Mol Biol 2012;929:377-389. doi:10.1007/978-1-62703-050-2_16 | PubMed 23007438 | Record and abstract read; chapter not read. Background only, cited by no rule. |
| S-17 | Gabrielsson J, Weiner D. Pharmacokinetic and Pharmacodynamic Data Analysis: Concepts and Applications (textbook). Sections cited by the PKNCA manual: 2.5.1 (derivation of clearance), 2.8.1 (linear trapezoidal rule), 2.8.3 (log-linear trapezoidal rule), 2.8.4 (strategies for estimation of λz). | Not accessible to the reader | The reader has not read the book. Section titles and page numbers are taken from S-01. That manual labels the edition "4th, 2000", which does not look like a matching edition and year, so the pairing and the page numbers must be checked against a physical copy. |
| S-18 | Gibaldi M, Perrier D. Pharmacokinetics, 2nd ed., revised and expanded (textbook, 1982). Cited by S-06 and S-07 for the NCA routines, without sections. | Not accessible to the reader | Not read. Cited at book level only. Section and page references still to be added by someone with the book. |
| S-19 | Rowland M, Tozer TN. Clinical Pharmacokinetics and Pharmacodynamics, 4th ed. 2011, pages 687-689 (trapezoidal AUC and AUMC), as cited by S-06. | Not accessible to the reader | Not read. Cited through S-06. |
| H | Hand checks: arithmetic done by the reader in a throwaway script kept outside the repository (section 5). | n/a | Own work. |
| D | Derivations by the reader (calculus, shown in `nca.md`). | n/a | Own work. |

PubMed bibliographic data came from the PubMed service (NLM); the records are cited by PubMed id and DOI above.

## 4. Looked at and deliberately not used

- The reference software's own documentation (section 2).
- Community forum threads and mailing-list archives about the reference software (may quote its documentation); not opened.
- The PKNCA source code (AGPL-3; the clean-room rule allows documentation only). Not opened.
- `private/` and `crates/`: not opened.
- Documentation of other commercial NCA tools beyond S-08 and S-09: not needed.

## 5. Hand checks performed by the reader

A small script kept in the session's scratch folder (not in the repository, not committed) reproduced numbers printed in public sources, to test the reader's reading of the rules. It is not project code. Results, to the printed precision:

- H1: the nine-point profile of the PKNCA AUC vignette (S-02:v05): AUClast, AUCall, tlast, λz, R², adjusted R², Clast,pred, half-life, span ratio, AUCinf,obs and AUCinf,pred all reproduced.
- H2: the three λz outcomes printed in the PKNCA half-life vignette (S-02:v06) for one subject of the public theophylline data (default selection; one point excluded; manual range): number of points, first time and λz reproduced to the printed digits.
- H3: the published comparison table of S-05 for one subject of the same public dataset (linear trapezoid, extravascular, best-fit λz): λz, adjusted R², half-life, AUClast, AUCinf (observed and predicted), AUMClast, AUMCinf (observed and predicted), percent extrapolated, CL/F, Vz/F and the MRT values were reproduced to the digits printed in the paper, by the formulas in `nca.md`. This is a statement about a published table, not a copy of it; see question Q-007.
- H4: for a pure mono-exponential profile, the log rule reproduces the analytic AUC and AUMC to rounding error, and the percentage error of the linear rule at 0.5, 2 and 4 half-lives per interval matches the figures in the abstract of S-12 (about 1%, 15.5%, 57.1%).
- H5: a search over random profiles found the discriminating λz profiles of worked example W6.
- H6: the rules of `specs/nca.md` applied to the 18 public profiles of T-003 reproduce every parameter in `oracle/expected/*.csv` that the script computes, to better than 1e-14 relative (details in `specs/nca.md` section 11). Reading the public oracle files is allowed; `private/` and `crates/` were not opened.

## 6. Internal evidence added after the oracle (card T-007, 2026-10-08)

The reader did not open `crates/`, `private/`, or the source code of PKNCA. The keys below are the project's own board and oracle files, read as reported by the agents who produced them. They back the tag `confirmed by oracle` in `specs/nca.md`.

| key | what | where | note |
|---|---|---|---|
| E-01 | Public oracle (card T-003): `theoph`, `theoph_linear`, `indometh`, `indometh_linear`, expected values computed with PKNCA 0.12.1 on R 4.5.2 with every option written explicitly, plus the facts recorded by the oracle agent (C0 added for Indometh, `tfirst`, no R² floor, dose constants) | `oracle/expected/*.csv`, `*.options.json`, `oracle/data/README.md`, `board/tasks/T-003.md` | 1080 values; the testkit re-derives them independently within 1e-6. PKNCA is used as an external tool, not as source code. |
| E-02 | Synthetic discriminating profiles (card T-005): `synthetic_lz`, `synthetic_lz_f1e3`, 4 subjects, 116 values each | `oracle/data/synthetic_lz.csv`, `oracle/expected/synthetic_lz*`, `board/tasks/T-005.md` | Settles the tie rule and the positive-slope filter order for PKNCA. The oracle agent reports that PKNCA's own function agrees on the strict comparison; this cannot be separated by data, so the reader relies on the engine and the oracle agent's report for it, not on a reading of the PKNCA source. |
| E-03 | Engine reports and test results (cards T-004a, T-004b, T-004c): `oracle_public` 21 of 21, `oracle_synthetic` 16 of 16, `cargo xtask conformance` 1312 of 1312 values within 1e-6 relative on 6 cases | `board/tasks/T-004a.md`, `T-004b.md`, `T-004c.md` | As reported by the engine and reviewer agents; the reader did not re-run the tests. |
| E-04 | Decisions of the orchestrator recorded on the cards: default `start_policy = auto`, BLQ-set points kept out of λz, Tlast and Tfirst, new options in `NcaOptions` only, missing or invalid dose gives NC instead of an error | `board/tasks/T-004a.md`, `T-004c.md` | Design decisions, not oracle facts. |

What the oracle does not cover is listed per rule in `specs/nca.md` (lines "Not covered by the oracle") and gathered in open item O-17.

## 7. Sources added for `specs/models.md` and `specs/fit.md` (card T-008, 2026-10-08)

Same rules as before: the reference software's own documentation was not used; textbooks are not accessible to the reader and are cited at book level; bibliographic records were checked through the Crossref service (DOI resolution), the papers themselves were not read.

| key | source | location | terms and how used |
|---|---|---|---|
| S-20 | pmxTools vignette "Drawing PK curves" (CRAN): catalogue of single-dose and steady-state one-compartment linear functions for IV bolus, IV infusion, zero-order and first-order oral input, with and without lag; parameterised by CL and V; states that the expressions follow Bertrand and Mentré (2008). Equations are not written out on the page. | `https://cran.r-project.org/web/packages/pmxTools/vignettes/pk-curves.html` (read through a page summary; first 100 000 of 145 000 characters) | GPL package documentation. Facts only (model list, argument names). Used for the model catalogue and the CL/V convention. |
| S-21 | rxode2 manual (CRAN): the theophylline example written as depot and central compartment ODEs with ka, cl, v. | `https://search.r-project.org/CRAN/refmans/rxode2/html/rxode2.html` (read through a search-result summary) | GPL-3 package documentation. Facts only. Used for the compartment structure and naming. |
| S-22 | PopED manual, model function `ff.PK.1.comp.oral.sd.CL` (parameter names CL, V, KA, Favail, DOSE). The page does not state the equation. | `https://rdrr.io/cran/PopED/man/ff.PK.1.comp.oral.sd.CL.html` | Names only; cited by no formula. |
| S-23 | Hartley HO. The modified Gauss-Newton method for the fitting of non-linear regression functions by least squares. Technometrics 1961;3(2):269-280. doi:10.1080/00401706.1961.10489945 | Crossref record checked | Paper not read. Cited for the idea of a step length along the Gauss-Newton direction. |
| S-24 | Levenberg K. A method for the solution of certain non-linear problems in least squares. Quarterly of Applied Mathematics 1944;2(2):164-168. doi:10.1090/qam/10666 | Crossref record checked | Paper not read. Cited for the damped normal equations. |
| S-25 | Marquardt DW. An algorithm for least-squares estimation of nonlinear parameters. J Soc Ind Appl Math 1963;11(2):431-441. doi:10.1137/0111030 | Crossref record checked | Paper not read. Cited for the scaled damping. |
| S-26 | R documentation `nls.control` and `confint` (stats package): maximum iterations 50, convergence tolerance 1e-5 on the relative-offset criterion, step-halving floor 1/1024, `confint` for non-linear fits profile-based, asymptotic (Wald) intervals by a separate function. | `https://stat.ethz.ch/R-manual/R-devel/library/stats/html/nls.control.html`, `.../confint.html` (page summaries) | R is GPL-2/3; facts about default values only. |
| S-27 | Akaike H. A new look at the statistical model identification. IEEE Trans Autom Control 1974;19(6):716-723. doi:10.1109/TAC.1974.1100705 | Crossref record checked | Paper not read. Cited for the principle of AIC. |
| S-28 | Textbooks on non-linear regression and pharmacokinetic data analysis: Gabrielsson and Weiner (S-17), Gibaldi and Perrier (S-18), and for the statistics of non-linear least squares Bates and Watts, Nonlinear Regression Analysis and Its Applications (Wiley 1988), and Draper and Smith, Applied Regression Analysis. | Not accessible to the reader | Not read. Cited at book level only; the statistics are derived (D) and the chapter references are left to someone holding the books (OM-05 in `models.md`). |
| S-29 | Schwarz G. Estimating the dimension of a model. Annals of Statistics 1978;6(2):461-464. doi:10.1214/aos/1176344136 | Crossref record checked (pages not returned by the service; the page range is from the reader's memory and should be checked) | Paper not read. Cited for the principle of the Bayesian/Schwarz criterion. |
| S-30 | Bertrand J, Mentré F. Mathematical expressions of the pharmacokinetic and pharmacodynamic models implemented in the Monolix software. 2008. | Cited by S-20 | Not located and not read; cited only through S-20. |

Hand checks added: H1 to H3 are listed in `specs/models.md` section 9 and `specs/fit.md` section 11 (closed forms against a numerical ODE integration; all worked-example numbers recomputed by a script outside the repository). `specs/models.md` and `specs/fit.md` use S-20 to S-30 only for conventions, names and default values, never for wording; no third-party text or number is copied, so no row was added to `ATTRIBUTION.md`.
