# Caladrius population-fit oracle, card T-152 item 4 (POP-INN-03, PX4): REFUSALS as expected-error rows.
#
# Every row is a request that the population fit must refuse before the first outer iteration, with the
# code and the words the message must contain. Rows that reuse an individual rule (FIT-ML-13c, FIT-ML-14,
# DAT-ERR-08) say so in `reuses`, and the data of the individual cases they come from is read from
# oracle/expected/fit/ml/errors.json and oracle/data/theoph.csv. A row may carry a `control`: the same
# request with the one thing changed that makes it acceptable; it must NOT be refused.
#
# `words_basis` says where the words come from: "spec_sentence" (the sentence is written in specs/population.md),
# "reused_sentence" (the sentence of the individual rule, oracle/expected/fit/ml/errors.json), "rule_names_option"
# (the rule says the error names the option or the label), "inferred" (the spec fixes the code, not the sentence:
# the words are the smallest ones that name what to change; a question for Abdou in the report).
#
# No fit is run and no number comes from a computation: the rows are text, copied from the sources named.
# Run: Rscript oracle/scripts/pop_errors.R   (writes oracle/expected/pop/errors.json and oracle/data/pop/errors/)
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])), "pop_lib.R"))
ROOT <- repo_root()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "errors.json")
DDIR <- file.path(ROOT, "oracle", "data", "pop", "errors")
unlink(DDIR, recursive = TRUE); dir.create(DDIR, recursive = TRUE, showWarnings = FALSE)

theoph <- read.csv(file.path(ROOT, "oracle", "data", "theoph.csv"), stringsAsFactors = FALSE)
ml <- jsonlite::fromJSON(file.path(ROOT, "oracle", "expected", "fit", "ml", "errors.json"), simplifyVector = FALSE)
ml_case <- function(id) { for (c in ml$cases) if (c$case == id) return(c); stop("no ml case ", id) }

arr <- function(x) as.list(x)
subj <- function(id, dose, time, y) list(id = id, dose = dose, time = arr(time), y = arr(y))
theoph_subj <- function(label, theoph_id, drop_t0 = FALSE, y0 = NULL) {
  d <- theoph[theoph$subject == theoph_id, ]
  if (drop_t0) d <- d[d$time > 0, ]
  y <- d$conc
  if (!is.null(y0)) y[1] <- y0
  subj(label, d$dose[1], d$time, y)
}
# initial values and Omega for the three oral models (the same units as the Theoph ML cases)
init_oral <- list(v = 32, cl = 2.8, ka = 1.5)
omega_diag <- function(...) { v <- c(...); lapply(seq_along(v), function(i) arr(ifelse(seq_along(v) == i, v[i], 0))) }

request <- function(model, initial, random_effects, omega, error_model = "combined", subjects = NULL, dataset_file = NULL, options = setNames(list(), character(0)), fixed = NULL, bounds = NULL, omega_shape = NULL) {
  r <- list(model = model, initial = initial, random_effects = arr(random_effects), omega = omega, error_model = error_model,
            options = options)
  if (!is.null(omega_shape)) r$omega_shape <- omega_shape
  if (!is.null(fixed)) r$fixed <- fixed
  if (!is.null(bounds)) r$bounds <- bounds
  if (!is.null(subjects)) r$subjects <- subjects
  if (!is.null(dataset_file)) r$dataset_file <- dataset_file
  r
}
# a request with one field replaced (NULL removes it)
mod <- function(r, ...) { ch <- list(...); for (n in names(ch)) r[n] <- list(ch[[n]]); r[!vapply(r, is.null, TRUE)] }
mod_opt <- function(r, ...) { ch <- list(...); for (n in names(ch)) r$options[n] <- list(ch[[n]]); r }

cases <- list()
add <- function(id, rule, description, req, code, words, basis, reuses = NULL, control = NULL, must_not_contain = NULL, note = NULL, expect = "refused", expect_flags = NULL) {
  c <- list(case = id, rule = rule, description = description, expect = expect, request = req)
  if (expect == "refused") c$error <- list(code = code, words = arr(words), words_basis = basis)
  if (!is.null(must_not_contain)) c$error$must_not_contain <- arr(must_not_contain)
  if (!is.null(reuses)) c$reuses <- reuses
  if (!is.null(control)) c$control <- control
  if (!is.null(note)) c$note <- note
  if (!is.null(expect_flags)) c$expect_flags <- arr(expect_flags)
  cases[[length(cases) + 1L]] <<- c
}

# ---- a base dataset of three subjects (Theoph 1 to 3, rows with time > 0), accepted by every error model
ok3 <- list(theoph_subj("S1", 1, TRUE), theoph_subj("S2", 2, TRUE), theoph_subj("S3", 3, TRUE))
base <- request("pk1.oral_1", init_oral, c("cl", "v"), omega_diag(0.09, 0.04), "combined", subjects = ok3)

# ================= rule 1, unbounded_likelihood (FIT-ML-13c per subject; PX4) =================
# subject S2 carries the observation 0 at time 0 where pk1.oral_1 predicts 0; S1 (0.74 at time 0) and S3 are fine
t0_subjects <- list(theoph_subj("S1", 1), subj("S2", ml_case("ml6e_y0_at_t0_combined_theoph2")$dose,
                                                 unlist(ml_case("ml6e_y0_at_t0_combined_theoph2")$time), unlist(ml_case("ml6e_y0_at_t0_combined_theoph2")$conc)),
                    theoph_subj("S3", 3, TRUE))
fit13c <- ml_case("ml6e_y0_at_t0_combined_theoph2")$error$words
for (em in c("combined", "combined_sd")) {
  r <- mod(base, error_model = em, subjects = t0_subjects)
  add(sprintf("pr1_unbounded_%s_s2", em), "POP-INN-03.1; PX4",
      sprintf("subject S2 has sample 0 at time 0 where pk1.oral_1 predicts 0, %s error: the likelihood of S2 is unbounded; S1 and S3 are fine (check made for every subject, the error names S2)", em),
      r, "unbounded_likelihood", c("in subject S2", unlist(fit13c)), "reused_sentence",
      reuses = "FIT-ML-13c (ml6e_y0_at_t0_combined_theoph2)",
      control = list(change = "error_model = additive", request = mod(r, error_model = "additive")),
      must_not_contain = c("S1", "S3"))
}
# the same row under proportional is NOT rule 1 (no lag): the prediction 0 gives sigma 0, rule 2 (non_positive_sigma), below.
# the leading zeros of a lagged route (ml6f): the data of the individual case in subject S2, S1 and S3 scaled so that their first sample is above 0
lagc <- ml_case("ml6f_leading_zero_combined_oral_1_lag")
lag_t <- unlist(lagc$time); lag_y <- unlist(lagc$conc); lag_dose <- lagc$dose
lag_subjects <- list(subj("S1", lag_dose, lag_t, lag_y * 1.1 + 0.2), subj("S2", lag_dose, lag_t, lag_y), subj("S3", lag_dose, lag_t, lag_y * 0.9 + 0.3))
lag_base <- request("pk1.oral_1_lag", list(v = 10, cl = 2, ka = 1, tlag = 0.3), c("cl", "v"), omega_diag(0.09, 0.04), "combined", subjects = lag_subjects)
lag_words <- lagc$error$words
for (em in c("combined", "combined_sd", "proportional")) {
  r <- mod(lag_base, error_model = em)
  add(sprintf("pr1_lag_leading_zero_%s", em), "POP-INN-03.1",
      sprintf("pk1.oral_1_lag, %s: S2 has sample 0 at time 0.5, and the domain of tlag (typical value, no random effect) meets [t0, t_nz) of S2", em),
      r, "unbounded_likelihood", c("in subject S2", unlist(lag_words)), "reused_sentence",
      reuses = "FIT-ML-13c (ml6f_leading_zero_*)",
      control = list(change = "error_model = additive", request = mod(r, error_model = "additive")),
      must_not_contain = c("S1", "S3"))
}
r <- mod(lag_base, error_model = "combined", fixed = list(tlag = 0.5))
add("pr1_lag_fixed_at_first_time", "POP-INN-03.1", "tlag fixed at 0.5 = the time of the zero sample: the prediction there is 0 for every trial, combined error (ml6f_fixed_at_time)",
    mod(r, initial = list(v = 10, cl = 2, ka = 1)), "unbounded_likelihood", c("in subject S2", unlist(lag_words)), "reused_sentence",
    reuses = "FIT-ML-13c (ml6f_fixed_at_time_combined_oral_1_lag)",
    control = list(change = "error_model = additive", request = mod(mod(r, initial = list(v = 10, cl = 2, ka = 1)), error_model = "additive")))
r <- mod(lag_base, error_model = "combined", bounds = list(tlag = list(upper = 0.5)))
add("pr1_lag_upper_equal_first_time", "POP-INN-03.1", "upper bound of tlag equal to the time of the zero sample (ml6f_upper_equal)",
    r, "unbounded_likelihood", c("in subject S2", unlist(lag_words)), "reused_sentence",
    reuses = "FIT-ML-13c (ml6f_upper_equal_combined_oral_1_lag)",
    control = list(change = "error_model = additive", request = mod(r, error_model = "additive")))
# a bound below the first sample time keeps the prediction above 0 at the zero sample: not refused (ml6f_lag_paths, upper_bound_below_t0)
add("pr1_lag_upper_below_first_time_accepted", "POP-INN-03.1", "upper bound of tlag below the time of the zero sample: the prediction there is above 0, the zero sample is an ordinary observation, the request is NOT refused (path upper_bound_below_t0 of ml6f_lag_paths.json converges)",
    mod(lag_base, error_model = "combined", bounds = list(tlag = list(upper = 0.4))), NULL, NULL, NULL,
    reuses = "FIT-ML-13c; oracle/expected/fit/ml/ml6f_lag_paths.json", expect = "accepted")

# ================= rule 2, non_positive_sigma =================
# proportional, pk1.oral_1, S1 has 0.74 at time 0 where the prediction is 0: sigma = b * 0 = 0 at eta = 0
sig_subjects <- list(theoph_subj("S1", 1), theoph_subj("S2", 2, TRUE), theoph_subj("S3", 3, TRUE))
r <- mod(base, error_model = "proportional", subjects = sig_subjects)
add("pr2_sigma_zero_proportional", "POP-INN-03.2",
    "proportional error, S1 has sample 0.74 at time 0 where pk1.oral_1 predicts 0: sigma = 0 at eta = 0 (not rule 1: no lag, and proportional is not in rule 1 without a lag)",
    r, "non_positive_sigma", c(unlist(ml_case("ml6b_sigma_zero_proportional")$error$words), "subject S1"), "spec_sentence",
    reuses = "FIT-ML-14b (ml6b_sigma_zero_proportional)",
    control = list(change = "S1 without its row at time 0", request = mod(r, subjects = ok3)),
    note = "the sentence of FIT-ML-14b with 'of subject S1' added after the time (POP-INN-03.2, decided 2026-11-08): the words are those of the individual rule plus 'subject S1'")

# ================= rule 3, too few data =================
one <- list(theoph_subj("S1", 1, TRUE))
add("pr3_too_few_subjects_px4", "POP-INN-03.3; PX4", "a dataset with one subject (PX4)",
    mod(base, subjects = one), "too_few_subjects", c("at least 2 subjects", "this dataset has 1"), "spec_sentence",
    reuses = "FIT-ML-14a (analogue)", control = list(change = "two subjects", request = mod(base, subjects = ok3[1:2])))
miss <- subj("S2", theoph$dose[theoph$subject == 2][1], theoph$time[theoph$subject == 2 & theoph$time > 0], rep(NA_real_, sum(theoph$subject == 2 & theoph$time > 0)))
add("pr3_too_few_subjects_all_missing", "POP-INN-03.3; POP-DAT-01", "two subjects, one with all observations missing: n_i = 0, it is not counted in m, so m = 1",
    mod(base, subjects = list(ok3[[1]], miss)), "too_few_subjects", c("at least 2 subjects", "this dataset has 1"), "spec_sentence",
    control = list(change = "S2 has observations", request = mod(base, subjects = ok3[1:2])))
# N <= P_theta,free + Q
iv <- function(em, n1, n2, re = "v", omega = omega_diag(0.09)) {
  t1 <- c(0.5, 1, 2, 4, 8); y1 <- c(9.6, 8.7, 7.2, 4.9, 2.3); t2 <- c(0.5, 1, 2, 4, 8); y2 <- c(10.4, 9.1, 7.5, 5.2, 2.5)
  request("pk1.iv_bolus", list(v = 10, k = 0.2), re, omega, em, subjects = list(subj("S1", 100, t1[seq_len(n1)], y1[seq_len(n1)]), subj("S2", 100, t2[seq_len(n2)], y2[seq_len(n2)])))
}
add("pr3_too_few_observations_additive", "POP-INN-03.3", "P_theta,free = 2 (v, k), Q = 1 (additive), N = 3: N <= P + Q",
    iv("additive", 2, 1), "too_few_observations", c("3 parameters", "from 3 observations"), "spec_sentence",
    reuses = "FIT-ML-14a (analogue, ml6c_too_few_additive)", control = list(change = "N = 4", request = iv("additive", 2, 2)))
add("pr3_too_few_observations_combined", "POP-INN-03.3", "P_theta,free = 2, Q = 2 (combined), N = 4: N <= P + Q",
    iv("combined", 2, 2), "too_few_observations", c("4 parameters", "from 4 observations"), "spec_sentence",
    reuses = "FIT-ML-14a (analogue, ml6c_too_few_combined)", control = list(change = "N = 5", request = iv("combined", 3, 2)))
add("pr3_omega_not_counted_accepted", "POP-INN-03.3", "N = 4 > P + Q = 3 with a full 2 x 2 block Omega (3 free elements): NOT refused (the elements of Omega are left out of the count on purpose); the covariance carries the flag few observations",
    mod(iv("additive", 2, 2, re = c("v", "k"), omega = list(arr(c(0.09, 0.01)), arr(c(0.01, 0.04)))), omega_shape = "block"), NULL, NULL, NULL, expect = "accepted", expect_flags = "few_observations",
    note = "the fit is run in full (not evaluate_only) and the array flags of the covariance block of the answer holds few_observations (POP-INN-03.3, POP-COV-03, POP-API-01 item 4); the elements of Omega are left out of the refusal count but not of the flag")

# ================= rule 4, inner_failed: no input can be built =================
cases[[length(cases) + 1L]] <- list(case = "pr4_inner_failed_sentence", rule = "POP-INN-03.4; PX4", expect = "refused", constructible = FALSE,
  description = "the inner problem of subject S3 does not converge at the starting parameters. No dataset and no request that the oracle can build and still be certain of this outcome is known: the row is the sentence only, to be exercised by a unit test of the engine with an injected failure (T-153), not by the oracle",
  request = NULL, error = list(code = "inner_failed", words = arr(c("inner problem of subject S3", "did not converge at the starting values", "check the starting values, the model and the data of that subject")), words_basis = "spec_sentence"))

# ================= rule 5, InvalidOption =================
inv <- function(id, desc, req, words, basis = "rule_names_option", control = NULL, note = NULL, rule = "POP-INN-03.5")
  add(id, rule, desc, req, "invalid_option", words, basis, control = control, note = note)
inv("pr5_unknown_method", "an unknown method", mod_opt(base, method = "mcem"), c("method"), control = list(change = "no method (the default)", request = base))
inv("pr5_unknown_error_model", "an unknown error_model", mod(base, error_model = "poisson"), c("error_model"), control = list(change = "combined", request = base))
inv("pr5_missing_error_model", "a missing error_model", mod(base, error_model = NULL), c("error_model"), control = list(change = "combined", request = base))
inv("pr5_unknown_ofv_convention", "an unknown ofv_convention", mod_opt(base, ofv_convention = "half"), c("ofv_convention"), control = list(change = "no_2pi", request = mod_opt(base, ofv_convention = "no_2pi")))
inv("pr5_unknown_omega_shape", "an unknown omega shape", mod(base, omega_shape = "banded"), c("omega"), control = list(change = "block", request = mod(base, omega_shape = "block", omega = list(arr(c(0.09, 0.01)), arr(c(0.01, 0.04))))))
inv("pr5_unknown_covariance_kind", "an unknown covariance kind", mod_opt(base, covariance = "jackknife"), c("covariance"), control = list(change = "sandwich", request = mod_opt(base, covariance = "sandwich")))
inv("pr5_missing_random_effects", "a missing random_effects", mod(base, random_effects = NULL), c("random_effects"), control = list(change = "cl, v", request = base))
inv("pr5_initial_outside_domain", "an initial value outside its domain (v = -1, a volume)", mod(base, initial = list(v = -1, cl = 2.8, ka = 1.5)), c("`initial`", "`v`"), basis = "spec_sentence",
    control = list(change = "v = 32", request = base), note = "the sentence of POP-INN-03.5: option `initial`: the typical value of `v` is -1, outside its domain; the backticks keep the word 'v' from matching inside 'invalid'")
inv("pr5_omega_not_pd_diagonal", "a negative variance on the diagonal of the starting Omega", mod(base, omega = omega_diag(0.09, -0.04)), c("omega"),
    control = list(change = "0.04", request = base))
inv("pr5_omega_not_pd_block", "a block starting Omega that is not positive definite (covariance 0.1 > sqrt(0.04 * 0.04))",
    mod(base, omega_shape = "block", omega = list(arr(c(0.04, 0.1)), arr(c(0.1, 0.04)))), c("omega"),
    control = list(change = "covariance 0.01", request = mod(base, omega_shape = "block", omega = list(arr(c(0.04, 0.01)), arr(c(0.01, 0.04))))))
inv("pr5_duplicate_random_effect", "random_effects lists the same parameter twice (added after review)", mod(base, random_effects = arr(c("cl", "cl")), omega = omega_diag(0.09, 0.04)), c("cl"),
    control = list(change = "cl, v", request = base))
# additive error: the combined error with this lagged model is itself refused by rule 1 (the zero sample of S2, as in the rows
# pr1_lag_leading_zero_*), so the row and its control would both be refused for that reason; with the additive error the only
# thing that differs between the row and its control is the random effect on tlag
lag_add <- mod(lag_base, error_model = "additive")
tl <- mod(lag_add, random_effects = arr(c("cl", "tlag")), omega = omega_diag(0.09, 0.04))
inv("pr5_random_effect_on_tlag_px4", "a random effect on tlag (PX4)", tl,
    c("random effect on `tlag`", "not available in this version", "give `tlag` as a shared typical value"), basis = "spec_sentence", rule = "POP-INN-03.5; POP-MOD-02; PX4",
    control = list(change = "tlag as a shared typical value", request = lag_add))
inv("pr5_random_effect_on_dur", "a random effect on dur (the zero-order duration, same reason as tlag)",
    mod(request("pk1.oral_0", list(v = 10, cl = 2, dur = 2), c("cl", "dur"), omega_diag(0.09, 0.04), "combined", subjects = ok3), random_effects = arr(c("cl", "dur"))),
    c("random effect on `dur`", "not available in this version", "give `dur` as a shared typical value"), basis = "spec_sentence", rule = "POP-INN-03.5; POP-MOD-02",
    control = list(change = "dur as a shared typical value", request = request("pk1.oral_0", list(v = 10, cl = 2, dur = 2), "cl", omega_diag(0.09), "combined", subjects = ok3)),
    note = "the sentence of POP-INN-03.5; the model name pk1.oral_0 is the zero-order oral model with parameter dur (see specs/models.md MOD-AB0-01)")

# ================= rule 6, data: DAT-ERR-08 and exclude_subjects =================
# an event-layout dataset of three subjects; subject 1 carries ADDL (unsupported_history, DAT-ERR-06): the sentence is that of oracle/expected/data/refuse_addl.json
csv <- c("ID,TIME,DV,AMT,EVID,ADDL",
         "1,0,,100,1,3", "1,0.5,3.1,,0,", "1,2,5.8,,0,", "1,6,4.2,,0,",
         "2,0,,120,1,0", "2,0.5,2.7,,0,", "2,2,5.2,,0,", "2,6,3.6,,0,",
         "3,0,,110,1,0", "3,0.5,3.0,,0,", "3,2,5.5,,0,", "3,6,3.9,,0,")
write_lines_lf(csv, file.path(DDIR, "refuse_addl_3.csv"))
dat <- read.csv(file.path(ROOT, "oracle", "data", "population", "refuse_addl.csv"))
stopifnot(identical(dat$ID[1:3], c(1L, 1L, 1L)), dat$ADDL[1] == 3)  # same first rows as the data oracle refuse_addl, hence the same row number in the sentence
dref <- jsonlite::fromJSON(file.path(ROOT, "oracle", "expected", "data", "refuse_addl.json"), simplifyVector = FALSE)
ev <- request("pk1.iv_bolus", list(v = 10, k = 0.2), "v", omega_diag(0.09), "combined", dataset_file = "data/pop/errors/refuse_addl_3.csv")
add("pr6_dataset_refusal_addl", "POP-INN-03.6; DAT-ERR-08; DAT-ERR-06",
    "event-layout dataset, subject 1 carries ADDL: the run refuses, naming the subject and the dataset's sentence; subjects 2 and 3 are fine",
    ev, dref$error[["code"]], c("subject 1", unlist(dref$error$words)), "reused_sentence",
    reuses = "DAT-ERR-08 (oracle/expected/data/refuse_addl.json: same first rows, hence the same row number)",
    control = list(change = "exclude_subjects = [\"1\"]", request = mod_opt(ev, exclude_subjects = arr("1"))),
    must_not_contain = c("subject 2", "subject 3"),
    note = "the code is the dataset's own code of the first affected subject (POP-INN-03.6, decided 2026-11-08); read by exact name, [[\"code\"]] (the operator $ matches names partly)")
stopifnot(identical(dref$error[["code"]], "unsupported_history"))
# several dose rows (POP-INN-03.7, DAT-USE-02): subject S2 has two dose rows, S1 and S3 one
dose_rows <- c("ID,TIME,DV,AMT,EVID",
               "S1,0,,100,1", "S1,0.5,9.0,,0", "S1,2,6.8,,0", "S1,6,3.1,,0",
               "S2,0,,120,1", "S2,0.5,10.9,,0", "S2,2,8.1,,0", "S2,3,,120,1", "S2,6,7.4,,0",
               "S3,0,,110,1", "S3,0.5,10.0,,0", "S3,2,7.4,,0", "S3,6,3.3,,0")
write_lines_lf(dose_rows, file.path(DDIR, "several_doses_3.csv"))
write_lines_lf(dose_rows[dose_rows != "S2,3,,120,1"], file.path(DDIR, "several_doses_control_3.csv"))
ev2 <- request("pk1.iv_bolus", list(v = 10, k = 0.2), "v", omega_diag(0.09), "combined", dataset_file = "data/pop/errors/several_doses_3.csv")
add("pr6_several_dose_rows", "POP-INN-03.7; DAT-USE-02", "event-layout dataset, subject S2 has two dose rows (times 0 and 3), S1 and S3 one: the run refuses, naming S2 only",
    ev2, "unsupported_history", c("subject S2", "2 dose rows", "single dose", "exclude_subjects"), "spec_sentence",
    reuses = "DAT-USE-01 (the NCA refuses more than one dose row); POP-DAT-01",
    control = list(change = "the second dose row of S2 removed", request = mod(ev2, dataset_file = "data/pop/errors/several_doses_control_3.csv")),
    must_not_contain = c("S1", "S3"))
add("pr6_exclude_unknown_label", "POP-INN-03.6; POP-DAT-01", "exclude_subjects names a label that is not in the dataset (a misspelt label would otherwise exclude nothing, silently)",
    mod_opt(base, exclude_subjects = arr("S9")), "invalid_option", c("S9"), "rule_names_option",
    control = list(change = "exclude_subjects = [\"S3\"]", request = mod_opt(base, exclude_subjects = arr("S3"))))
add("pr6_exclude_unknown_label_among_valid", "POP-INN-03.6", "one valid and one misspelt label: still refused, naming the misspelt one",
    mod_opt(base, exclude_subjects = arr(c("S3", "s2"))), "invalid_option", c("s2"), "rule_names_option",
    control = list(change = "the misspelt label removed (S3 only)", request = mod_opt(base, exclude_subjects = arr("S3"))),
    must_not_contain = c("S3"))

out <- list(schema = 1L, kind = "pop_errors", generated_by = "oracle/scripts/pop_errors.R", versions = versions_list(),
            note = "expected-error rows; 'request' is the population request in the vocabulary of specs/population.md (the adapter of crates/caladrius-fit/tests/oracle_pop.rs maps it); expect = refused: the error code (if given) is checked first, then every word of 'words' must occur in the message and none of 'must_not_contain'; 'control' is the same request with the stated change and must not be refused; expect = accepted: the request must not be refused",
            cases = cases)
write_json_lf(out, OUT, digits = NA)
n_ref <- sum(vapply(cases, function(c) c$expect == "refused", TRUE))
cat(sprintf("pop_errors: %d rows (%d refused, %d accepted), %d with a control; sha256 %s\n", length(cases), n_ref, length(cases) - n_ref,
            sum(vapply(cases, function(c) !is.null(c$control), TRUE)), sha256_file(OUT)))
