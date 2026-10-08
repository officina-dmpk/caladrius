# Attribution

Every third-party asset (dataset, text excerpt, icon, font) used in this repository is listed here, in the same commit that adds it.

| asset | origin | license | used in |
|---|---|---|---|
| `oracle/data/theoph.csv` (Theoph theophylline profiles, 12 subjects) | `Theoph` data frame of the R `datasets` package (R 4.5.2); data from Boeckmann, Sheiner and Beal (1994), NONMEM Users Guide Part V | distributed with R under GPL-2 or GPL-3; see Q-004 in `board/QUESTIONS.md` | `oracle/`, `caladrius-testkit`, `caladrius-nca` tests |
| `oracle/data/indometh.csv` (Indometh indomethacin IV profiles, 6 subjects; dose 25 mg is a test constant) | `Indometh` data frame of the R `datasets` package (R 4.5.2); data from Kwan et al. (1976), J. Pharmacokinet. Biopharm. 4:255-280 | distributed with R under GPL-2 or GPL-3; see Q-004 | `oracle/`, `caladrius-testkit`, `caladrius-nca` tests |
| `oracle/expected/*.csv`, `*.options.json` (expected NCA values) | computed with PKNCA 0.12.1 on R 4.5.2 by `oracle/scripts/nca_pknca.R`; PKNCA is used as an external tool, none of its code or text is copied | PKNCA is AGPL-3; generated numbers only | `caladrius-nca` oracle tests |
| printed example numbers: the nine-point AUC profile and its results; half-life results for one public theophylline subject (numbers only, no text or code) | PKNCA 0.12.1 vignettes "AUC Calculation with PKNCA" and "Half-Life Calculation" (CRAN) | PKNCA is AGPL-3; a handful of printed numerical results, cited as documented targets | `specs/nca.md` worked examples W1 and W8 |
