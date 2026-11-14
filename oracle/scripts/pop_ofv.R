# Caladrius population-fit oracle, card T-152 items 2 and 3 (POP-ORA-02 reference side, POP-DIA-*):
# objectives at fixed parameters, nonlinear models, and the diagnostics, in double precision.
#
# Models: pk1.iv_bolus, pk1.oral_1, pk2.iv_bolus (clearance set), each with the four error models;
# data: seeded synthetic profiles, Theoph (pk1.oral_1, rows with time > 0) and Indometh (pk2.iv_bolus).
#
# THE CONDITIONAL MODE is found by a route that is not the engine's: stats::optim (BFGS, double
# precision, plain R code for the model) from three starts, which must agree; then a Newton polish
# with exact 256-bit second-order jets (pop_lib.R) until the largest component of the gradient of
# h_i is below 1e-70; the gradient at the polished point is verified at 256 bits and its size is
# written in the case. Everything is then rounded ONCE to double for the file.
#
# Per subject: eta_hat, OFV_i for the five forms (linearised POP-OBJ-02, FO POP-OBJ-03, laplace_full,
# laplace_expected, laplace_no_variance POP-OBJ-04) in both conventions of POP-OBJ-05 (full, no_2pi),
# the total; and the diagnostics of POP-DIA-01 to -04 (PRED, IPRED, IWRES, E, diag C, CWRES in
# the three forms, shrinkage with both divisors, AIC and BIC with k written out).
#
# Run: Rscript oracle/scripts/pop_ofv.R   (writes oracle/expected/pop/ofv/*.json, LF endings)
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])), "pop_lib.R"))
ROOT <- repo_root()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "ofv")
dir.create(OUT, recursive = TRUE, showWarnings = FALSE)
ONLY <- Sys.getenv("POP_OFV_ONLY", "")   # development filter (a regular expression on the case id); unset for the real run
if (!nzchar(ONLY)) unlink(list.files(OUT, pattern = "\\.json$", full.names = TRUE))
arr <- function(x) as.list(x)
D17 <- function(x) dbl(x)                       # one rounding, from 256 bits to the nearest double
dvec <- function(v) arr(vapply(seq_along(v), function(i) dbl(v[i]), 0))
FORMS <- c("linearised", "fo", "laplace_full", "laplace_expected", "laplace_no_variance")

# ------------------------------------------------------------------ the double-precision mode (not the engine's route)
mode_by_optim <- function(spec, subj) {
  q <- spec$q
  P0 <- lapply(spec$theta, function(x) asNumeric(x)); Om <- matrix(0, q, q)
  for (i in seq_len(q)) for (j in seq_len(q)) Om[i, j] <- asNumeric(spec$omega[[i]][j])
  Oi <- solve(Om); y <- as.numeric(unlist(subj$y)); tm <- as.numeric(unlist(subj$time)); D <- as.numeric(subj$dose)
  a <- asNumeric(spec$xi$a); b <- asNumeric(spec$xi$b)
  h <- function(eta) {
    P <- P0; for (k in seq_len(q)) P[[spec$re[k]]] <- P0[[spec$re[k]]] * exp(eta[k])
    yh <- pred_dbl(spec$model, P, D, tm); s <- sd_dbl(spec$error_model, a, b, yh)
    sum(2 * log(s) + ((y - yh) / s)^2) + sum(eta * (Oi %*% eta))
  }
  w <- sqrt(diag(Om))
  starts <- list(rep(0, q), 0.3 * w, -0.3 * w)
  fits <- lapply(starts, function(s0) optim(s0, h, method = "BFGS", control = list(reltol = 1e-14, maxit = 1000)))
  vals <- vapply(fits, function(f) f$value, 0); best <- fits[[which.min(vals)]]
  spread <- max(vapply(fits, function(f) max(abs(f$par - best$par)), 0))
  list(eta = best$par, start_spread = spread, h_values = vals)
}

# ------------------------------------------------------------------ one subject
eval_subject_full <- function(spec, subj) {
  q <- spec$q
  mo <- mode_by_optim(spec, subj)
  m <- find_mode(spec, subj, start = mpfr(mo$eta, BITS))
  pc <- form_pieces(spec, subj, m$eta, NULL)
  lf <- lin_form(spec, pc); lap <- laplace_forms(spec, pc)
  pc0 <- form_pieces(spec, subj, rep(list(ZERO), q), NULL); fo <- lin_form(spec, pc0)
  forms <- list(linearised = lf$ofv, fo = fo$ofv, laplace_full = lap$laplace_full,
                laplace_expected = lap$laplace_expected, laplace_no_variance = lap$laplace_no_variance)
  # sensitivity of the objective to an error in eta_hat: central change over a step 1e-6
  dl <- mpfr("1e-6", BITS); sens <- 0
  for (k in seq_len(q)) {
    up <- m$eta; dn <- m$eta; up[[k]] <- up[[k]] + dl; dn[[k]] <- dn[[k]] - dl
    ou <- lin_form(spec, form_pieces(spec, subj, up, NULL))$ofv; od <- lin_form(spec, form_pieces(spec, subj, dn, NULL))$ofv
    sens <- max(sens, asNumeric(abs(ou - od) / (2 * dl)))
  }
  cw <- cwres_forms(lf)
  n <- pc$n
  K <- make_K(NULL)
  pred <- mvec_(predict_subject(spec, subj, rep(list(ZERO), q), K))
  ipred <- pc$yhat
  s2hat <- pc$s2
  iwres <- (pc$y - ipred) / sqrt(s2hat)
  list(eta = m$eta, forms = forms, n = n, gradmax = m$gradmax, start_spread = mo$start_spread, sens = sens,
       diag = list(pred = pred, ipred = ipred, iwres = iwres, e = lf$E, c_diag = mvec_(lapply(seq_len(n), function(j) lf$C[[j]][j])),
                   cwres_diagonal = cw$diagonal, cwres_cholesky = cw$cholesky, cwres_symmetric = cw$symmetric,
                   symmetric_residual = cw$symmetric_residual, sum_w2 = lf$dist, logdet_c = lf$logdet,
                   sigma_hat = sqrt(s2hat)))
}

sd_div <- function(x, div) { x <- as.numeric(x); sqrt(sum((x - mean(x))^2) / div) }

# ------------------------------------------------------------------ case writer
n_error_params <- c(additive = 1L, proportional = 1L, combined = 2L, combined_sd = 2L)
write_ofv_case <- function(id, source, desc, spec, subjects, theta_in, omega_in, xi_in, re_in) {
  if (nzchar(ONLY) && !grepl(ONLY, id)) return(invisible(NULL))
  t0 <- Sys.time()
  res <- lapply(subjects, function(s) eval_subject_full(spec, s))
  m <- length(subjects); nobs <- sum(vapply(res, function(r) r$n, 0)); q <- spec$q
  tot <- function(nm) { s <- ZERO; for (r in res) s <- s + r$forms[[nm]]; s }
  per_subject <- lapply(seq_len(m), function(i) {
    r <- res[[i]]; n2 <- r$n * LN_2PI
    list(id = subjects[[i]]$id, n_obs = r$n, eta_hat = dvec(mvec_(r$eta)),
         ofv_full = lapply(r$forms, D17), ofv_no_2pi = lapply(r$forms, function(f) D17(f - n2)),
         diagnostics = list(
           pred = dvec(r$diag$pred), ipred = dvec(r$diag$ipred), iwres = dvec(r$diag$iwres), sigma_hat = dvec(r$diag$sigma_hat),
           e = dvec(r$diag$e), c_diag = dvec(r$diag$c_diag), cwres_diagonal = dvec(r$diag$cwres_diagonal),
           cwres_cholesky = dvec(r$diag$cwres_cholesky), cwres_symmetric = dvec(r$diag$cwres_symmetric),
           sum_w2 = D17(r$diag$sum_w2), logdet_c = D17(r$diag$logdet_c)))
  })
  total_full <- lapply(setNames(FORMS, FORMS), function(nm) D17(tot(nm)))
  total_no2pi <- lapply(setNames(FORMS, FORMS), function(nm) D17(tot(nm) - nobs * LN_2PI))
  # population diagnostics
  etas <- lapply(seq_len(q), function(k) vapply(res, function(r) asNumeric(r$eta[[k]]), 0))
  om_sd <- vapply(seq_len(q), function(k) sqrt(asNumeric(spec$omega[[k]][k])), 0)
  shr <- function(div) arr(vapply(seq_len(q), function(k) 1 - sd_div(etas[[k]], if (div == "m1") m - 1 else m) / om_sd[k], 0))
  sdv <- function(div) arr(vapply(seq_len(q), function(k) sd_div(etas[[k]], if (div == "m1") m - 1 else m), 0))
  iw <- as.numeric(unlist(lapply(res, function(r) asNumeric(r$diag$iwres))))
  diag_omega <- all(vapply(seq_len(q), function(i) all(vapply(seq_len(q), function(j) i == j || asNumeric(spec$omega[[i]][j]) == 0, TRUE)), TRUE))
  k_omega <- if (diag_omega) q else q * (q + 1) / 2
  k_theta <- length(spec$theta); k_xi <- n_error_params[[spec$error_model]]; kk <- k_theta + k_omega + k_xi
  ic <- lapply(setNames(FORMS, FORMS), function(nm) {
    o <- tot(nm)
    list(aic = D17(o + 2 * kk), bic_n_observations = D17(o + kk * log(mpfr(nobs, BITS))), bic_n_subjects = D17(o + kk * log(mpfr(m, BITS))))
  })
  doc <- list(schema = 1L, kind = "pop_ofv", id = id, source = source, description = desc,
              model = list(name = spec$model, random_effects = arr(re_in), theta = theta_in,
                           omega = lapply(omega_in, function(r) arr(unlist(r))), error_model = spec$error_model, xi = xi_in),
              subjects = lapply(subjects, function(s) list(id = s$id, dose = s$dose, time = arr(s$time), y = arr(unlist(s$y)))),
              mode_route = list(method = "optim BFGS (double) from 3 starts, then Newton with exact 256-bit jets",
                                start_spread_max_abs = max(vapply(res, function(r) r$start_spread, 0)),
                                gradient_max_abs_256bit = max(vapply(res, function(r) asNumeric(r$gradmax), 0)),
                                gradient_bound = "1e-70"),
              expected = list(n_subjects = m, n_obs = nobs, n_ln_2pi = D17(nobs * LN_2PI), per_subject = per_subject,
                              total_full = total_full, total_no_2pi = total_no2pi),
              diagnostics_population = list(
                eta_shrinkage_divisor_m_minus_1 = shr("m1"), eta_shrinkage_divisor_m = shr("m"),
                sd_eta_hat_divisor_m_minus_1 = sdv("m1"), sd_eta_hat_divisor_m = sdv("m"), omega_sd = arr(om_sd),
                sd_iwres_divisor_n_minus_1 = sd_div(iw, length(iw) - 1), sd_iwres_divisor_n = sd_div(iw, length(iw)),
                eps_shrinkage_divisor_n_minus_1 = 1 - sd_div(iw, length(iw) - 1), eps_shrinkage_divisor_n = 1 - sd_div(iw, length(iw)),
                k = list(theta = k_theta, omega = k_omega, xi = unname(k_xi), total = unname(kk),
                         note = "theta = typical values of the model, omega = diagonal elements or q(q+1)/2 for a block, xi = error parameters"),
                information_criteria_full = ic, bic_note = "the count in the BIC is open (OP-05): both are written"),
              checks = list(
                eta_perturbation_ofv_slope_max = max(vapply(res, function(r) r$sens, 0)),
                symmetric_root_residual_max = max(vapply(res, function(r) r$diag$symmetric_residual, 0)),
                identity_linearised_vs_no_variance_max_abs = max(vapply(res, function(r) asNumeric(abs(r$forms$linearised - r$forms$laplace_no_variance)), 0)),
                rounding = "double values are the nearest double to the 256-bit value"))
  write_json_lf(doc, file.path(OUT, paste0(id, ".json")))
  cat(sprintf("%-26s subjects=%d obs=%d  spread=%.1e  grad=%.1e  %.0fs\n", id, m, nobs, doc$mode_route$start_spread_max_abs,
              doc$mode_route$gradient_max_abs_256bit, as.numeric(difftime(Sys.time(), t0, units = "secs"))))
  invisible(res)
}

SUBJ <- function(id, time, y, dose) list(id = id, time = time, y = y, dose = dose)
ERRORS <- list(additive = "a", proportional = "b", combined = c("a", "b"), combined_sd = c("a", "b"))

# ------------------------------------------------------------------ synthetic cases
TH_IV <- list(v = "50", cl = "5"); TH_OR <- list(v = "50", cl = "5", ka = "1.2")
TH_PK2 <- list(cl = "5", vc = "20", q = "8", vp = "40")
IVT <- c("0.5", "1", "2", "4", "8", "12"); ORT <- c("0.5", "1", "2", "4", "8", "16"); PKT <- c("0.25", "1", "2", "4", "10", "24")
OMD2 <- list(list("0.09", "0"), list("0", "0.04")); OMB2 <- list(list("0.09", "0.03"), list("0.03", "0.04"))
XI <- list(additive = list(a = "0.12"), proportional = list(b = "0.1"), combined = list(a = "0.05", b = "0.1"), combined_sd = list(a = "0.03", b = "0.08"))
syn_specs <- list(
  list(key = "iv", model = "pk1.iv_bolus", theta = TH_IV, re = c("cl", "v"), omega = function(e) OMD2, times = IVT, dose = "100", seed0 = 152400L),
  list(key = "oral", model = "pk1.oral_1", theta = TH_OR, re = c("cl", "v"), omega = function(e) if (e %in% c("combined", "combined_sd")) OMB2 else OMD2,
       times = ORT, dose = "100", seed0 = 152410L),
  list(key = "pk2", model = "pk2.iv_bolus", theta = TH_PK2, re = c("cl", "vc"), omega = function(e) OMD2, times = PKT, dose = "100", seed0 = 152420L))
for (ss in syn_specs) for (ei in seq_along(XI)) {
  e <- names(XI)[ei]; xi <- XI[[e]]; omega <- ss$omega(e); seed <- ss$seed0 + ei
  subjects <- simulate_subjects(ss$model, ss$theta, ss$re, omega, e, xi, ss$dose, ss$times, 5L, seed)
  spec <- make_spec(ss$model, ss$theta, ss$re, omega, e, xi)
  write_ofv_case(sprintf("syn_%s_%s", ss$key, e), list(kind = "synthetic", seed = seed, rng = "Mersenne-Twister/Inversion/Rejection", significant_digits = 3L),
                 sprintf("%s, %s error, random effects on %s, 5 seeded subjects", ss$model, e, paste(ss$re, collapse = " and ")),
                 spec, subjects, ss$theta, omega, xi, ss$re)
}

# ------------------------------------------------------------------ Theoph and Indometh
read_profile <- function(file) {
  x <- read.csv(file, colClasses = "character")
  ids <- unique(x$subject)
  lapply(ids, function(i) { r <- x[x$subject == i, ]; list(id = i, time = r$time, y = r$conc, dose = r$dose[1]) })
}
theoph <- lapply(read_profile(file.path(ROOT, "oracle", "data", "theoph.csv")), function(s) {
  keep <- as.numeric(s$time) > 0; list(id = s$id, time = s$time[keep], y = s$y[keep], dose = s$dose) })
indometh <- read_profile(file.path(ROOT, "oracle", "data", "indometh.csv"))
TH_THEO <- list(v = "32", cl = "2.8", ka = "1.5"); OM_THEO <- list(list("0.09", "0", "0"), list("0", "0.04", "0"), list("0", "0", "0.25"))
XI_THEO <- list(additive = list(a = "0.7"), proportional = list(b = "0.12"), combined = list(a = "0.4", b = "0.1"), combined_sd = list(a = "0.3", b = "0.1"))
TH_INDO <- list(cl = "8", vc = "9", q = "5.5", vp = "16"); OM_INDO <- list(list("0.09", "0"), list("0", "0.04"))
XI_INDO <- list(additive = list(a = "0.05"), proportional = list(b = "0.15"), combined = list(a = "0.03", b = "0.1"), combined_sd = list(a = "0.02", b = "0.1"))
for (e in names(XI_THEO)) {
  spec <- make_spec("pk1.oral_1", TH_THEO, c("cl", "v", "ka"), OM_THEO, e, XI_THEO[[e]])
  write_ofv_case(paste0("theoph_", e), list(kind = "file", file = "oracle/data/theoph.csv", rows = "time > 0 (the time-0 samples are dropped: y = 0 there)"),
                 sprintf("Theoph, pk1.oral_1, %s error, random effects on cl, v, ka (diagonal), 12 subjects", e),
                 spec, theoph, TH_THEO, OM_THEO, XI_THEO[[e]], c("cl", "v", "ka"))
}
for (e in names(XI_INDO)) {
  spec <- make_spec("pk2.iv_bolus", TH_INDO, c("cl", "vc"), OM_INDO, e, XI_INDO[[e]])
  write_ofv_case(paste0("indometh_", e), list(kind = "file", file = "oracle/data/indometh.csv", rows = "all"),
                 sprintf("Indometh, pk2.iv_bolus (clearance set), %s error, random effects on cl and vc (diagonal), 6 subjects", e),
                 spec, indometh, TH_INDO, OM_INDO, XI_INDO[[e]], c("cl", "vc"))
}

write_json_lf(list(schema = 1L, kind = "pop_ofv_options", generated_by = "oracle/scripts/pop_ofv.R", versions = versions_list(),
                   mode = "optim BFGS from three starts (0, +0.3 omega, -0.3 omega), reltol 1e-14; Newton with 256-bit jets to a gradient below 1e-70",
                   seeds = "synthetic data: set.seed(seed) in Mersenne-Twister/Inversion/Rejection, seeds in the cases"),
              file.path(OUT, "options.json"))
cat("done\n")
