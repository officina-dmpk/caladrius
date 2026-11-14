# Specifications

Behaviour and UX specifications written by the `reader` agent in its own words and formulas (see `AGENTS.md` section 6). Implementer agents read these files and the oracle, nothing else.

Every rule carries a status tag:

- `confirmed by oracle`: a versioned test reproduces it.
- `documented, untested`: found in an allowed source, not yet tested.
- `assumed`: our best guess; becomes a test task before being relied on.
- `later`: a model or convention that an allowed source describes but that lies outside the current scope (`AGENTS.md` section 1); listed with its source pages, not specified, no test; not counted in the status tables.

Files: `nca.md` (T-002), `models.md` (T-008; one compartment, sections 1 to 10; two compartments, section 11, T-031: `MOD-2C-*`; synced with the two-compartment oracle and engine by T-035, where 18 of its 25 rules are `confirmed by oracle`; T-046 added page citations from the paper S-36 (Bertrand and Mentré 2008) as a second source to the rules it covers, section 12 on multiple dosing and steady state, `MOD-MD-*`, 16 rules; synced with the multiple-dosing oracle (T-047) and engine (T-049) by T-050, where 14 of them are `confirmed by oracle` and 2 `assumed`, and one printed branch of S-36 is recorded as D-05; section 13, `MOD-LT-*`, three compartments, Michaelis-Menten elimination and the effect compartment, status `later`; and section 14, `MOD-3C-*`, three-compartment models, 27 rules, T-140, 18 `documented, untested` and 9 `assumed`, open items OM-19 to OM-29 for the oracle card T-142), `fit.md` (weighted least squares, sections 2 to 12; section 13, T-147: maximum likelihood with an estimated error model and IWRES, `FIT-ML-*`, 21 rules, 13 `assumed` and 8 `documented, untested`, open items `OF-12` to `OF-21`), `data.md` (T-147: the population dataset, an event table with several subjects, `DAT-*`, with open items `OD-01` to `OD-07` for the oracle), `ux.md`, `differences.md` (documented method differences with PKNCA, with R, and with one published formula, D-05), `sources.md` (sources consulted and their terms of use), `population.md` (T-162: the population fit, FOCE with interaction and FO as its special case, `POP-*`, status of every rule `assumed` or `documented, untested`, open items `OP-01` to `OP-14`; synced by T-166 with the Q-025 decisions, plain FOCE `POP-OBJ-06` and the vocabulary `POP-API-01`).
