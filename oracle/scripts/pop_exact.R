# Caladrius population-fit oracle, card T-152 item 1 (POP-ORA-01): the exact identification suite,
# at 256 bits with Rmpfr, from decimal strings, needing no population software.
#
#   family a_linear      linear model (test-only identity link, POP-MOD-05), additive error: the
#                        marginal likelihood is exact; every form equals it to 256-bit rounding. The
#                        reference value is an INDEPENDENT closed form (C = Z Omega Z' + s2 I by
#                        Cholesky, posterior mean eta = Omega Z' C^-1 (y - a)), not the Laplace code.
#   family b_nonlinear   pk1/pk2 closed forms, additive error: linearised = laplace_no_variance =
#                        laplace_expected to rounding (POP-OBJ-04); laplace_full, FO and the exact
#                        marginal differ. The exact marginal (q = 1) is by trapezoidal quadrature with
#                        a stated error bound.
#   family c_frozen      proportional / combined error with sigma frozen (at the mode, or at the
#                        population prediction): linearised = laplace_no_variance to rounding.
#   family d_interaction sigma depends on eta (PX2 and more): the forms split; each is its own
#                        expected value.
#
# Run: Rscript oracle/scripts/pop_exact.R   (writes oracle/expected/pop/exact/*.json, LF endings)
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])), "pop_lib.R"))
ROOT <- repo_root()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "exact")
dir.create(OUT, recursive = TRUE, showWarnings = FALSE)
unlink(list.files(OUT, pattern = "\\.json$", full.names = TRUE))

SIG <- 50L                       # significant digits written for every expected value
arr <- function(x) as.list(x)
S <- function(x) fmt_mp(x, SIG)
D3 <- function(x) format(asNumeric(x), digits = 3)
mvec <- function(lst) { out <- mpfr(rep(0, length(lst)), BITS); for (i in seq_along(lst)) out[i] <- lst[[i]]; out }

# ------------------------------------------------------------------ evaluation of one subject
lin_closed_form <- function(spec, subj) {
  # independent route (family a): marginal N(a, C), C = Z Omega Z' + s2 I
  n <- length(subj$time); q <- spec$q
  a <- mvec(predict_subject(spec, subj, rep(list(ZERO), q), make_K(NULL)))
  Z <- lapply(seq_len(n), function(j) { tt <- Mp(subj$time[[j]]); mvec(lapply(spec$z_powers, function(pw) if (pw == 0) ONE else tt^pw)) })
  s2 <- spec$xi$a * spec$xi$a
  ZO <- lapply(seq_len(n), function(j) mat_vec(spec$omega, Z[[j]]))
  C <- mat_zero(n, n)
  for (i in seq_len(n)) for (j in seq_len(n)) C[[i]][j] <- sum(Z[[i]] * ZO[[j]]) + (if (i == j) s2 else ZERO)
  L <- chol_mp(C); r <- mvec(lapply(subj$y, Mp)) - a
  w <- fwd_mp(L, r); Cinv_r <- back_mp(L, w)
  Ztc <- mvec(lapply(seq_len(q), function(k) sum(mvec(lapply(seq_len(n), function(j) Z[[j]][k] * Cinv_r[j])))))
  list(ofv = n * LN_2PI + logdet_chol(L) + sum(w * w), eta = mat_vec(spec$omega, Ztc))
}

eval_subject <- function(spec, subj, frozen = "none", do_marginal = FALSE, h_factor = 0.6) {
  q <- spec$q
  m <- if (frozen == "none") find_mode(spec, subj) else find_mode_frozen(spec, subj, frozen)
  fx <- if (frozen == "none") NULL else m$sig2
  pc <- form_pieces(spec, subj, m$eta, fx)
  lf <- lin_form(spec, pc); lap <- laplace_forms(spec, pc)
  pc0 <- form_pieces(spec, subj, rep(list(ZERO), q), NULL); fo <- lin_form(spec, pc0)
  forms <- list(linearised = lf$ofv, fo = fo$ofv, laplace_full = lap$laplace_full,
                laplace_expected = lap$laplace_expected, laplace_no_variance = lap$laplace_no_variance)
  out <- list(eta = m$eta, forms = forms, gradmax = m$gradmax, n = pc$n)
  if (do_marginal) {
    H <- hess_matrix(m$parts$h)[[1]][1]
    spost <- sqrt(2 / H); h <- min(h_factor * spost, sqrt(spec$omega[[1]][1]) / 2)
    out$marginal <- marginal_q1(spec, subj, h)
    out$forms$exact_marginal <- out$marginal$ofv
  }
  out
}

# ------------------------------------------------------------------ case writer
maxabs <- function(res, f) max(vapply(res, function(r) asNumeric(f(r)), 0))
write_case <- function(id, family, description, spec, subjects, frozen = "none", do_marginal = FALSE, extra_checks = NULL,
                       theta_in, omega_in, xi_in, re_in, z_powers = NULL) {
  t0 <- Sys.time()
  res <- lapply(subjects, function(s) eval_subject(spec, s, frozen, do_marginal))
  nsub <- length(subjects); nobs <- sum(vapply(res, function(r) r$n, 0))
  form_names <- names(res[[1]]$forms)
  tot <- function(nm) { s <- ZERO; for (r in res) s <- s + r$forms[[nm]]; s }
  per_subject <- lapply(seq_len(nsub), function(i) {
    r <- res[[i]]
    list(id = subjects[[i]]$id, eta_hat = arr(fmt_vec(mvec(r$eta), SIG)), ofv_full = lapply(r$forms, S))
  })
  n2pi <- nobs * LN_2PI
  totals <- lapply(setNames(form_names, form_names), function(nm) S(tot(nm)))
  totals_no2pi <- lapply(setNames(form_names, form_names), function(nm) S(tot(nm) - n2pi))
  checks <- list(
    mode_gradient_max_abs_256bit = D3(maxabs(res, function(r) r$gradmax)),
    identity_linearised_vs_no_variance_max_abs = D3(maxabs(res, function(r) abs(r$forms$linearised - r$forms$laplace_no_variance))),
    identity_linearised_vs_expected_max_abs = D3(maxabs(res, function(r) abs(r$forms$linearised - r$forms$laplace_expected))),
    laplace_full_minus_linearised_max_abs = D3(maxabs(res, function(r) abs(r$forms$laplace_full - r$forms$linearised))),
    fo_minus_linearised_max_abs = D3(maxabs(res, function(r) abs(r$forms$fo - r$forms$linearised))),
    unit_roundoff_256bit = D3(2^-256))
  if (!is.null(extra_checks)) checks <- c(checks, extra_checks(res, subjects))
  if (do_marginal) {
    checks$exact_marginal_minus_linearised_max_abs <- D3(maxabs(res, function(r) abs(r$forms$exact_marginal - r$forms$linearised)))
    checks$exact_marginal <- list(
      method = "trapezoidal rule on [-13 sd, 13 sd] of the prior, steps h and h/2, h = min(0.6 s_post, sd/2), 256-bit arithmetic",
      ofv_total_error_bound = D3(sum(vapply(res, function(r) asNumeric(r$marginal$ofv_error_bound), 0))),
      per_subject = lapply(res, function(r) list(
        ofv_error_bound = D3(r$marginal$ofv_error_bound), discretisation_rel = D3(r$marginal$discretisation_rel),
        truncation_rel = D3(r$marginal$truncation_rel), nodes = r$marginal$nodes, step = D3(r$marginal$step),
        edge_log_ratio = D3(r$marginal$edge_log_ratio))))
  }
  doc <- list(schema = 1L, kind = "pop_exact", id = id, family = family, description = description,
              precision_bits = BITS, digits = SIG, frozen_sigma = frozen,
              model = list(name = spec$model, random_effects = arr(re_in), theta = theta_in,
                           omega = lapply(omega_in, function(r) arr(unlist(r))), error_model = spec$error_model, xi = xi_in,
                           z_powers = if (is.null(z_powers)) NULL else arr(z_powers)),
              subjects = lapply(subjects, function(s) list(id = s$id, dose = s$dose, time = arr(s$time), y = arr(unlist(s$y)))),
              expected = list(n_obs = nobs, n_ln_2pi = S(n2pi), conventions = list(full = "OFV as computed", no_2pi = "OFV - n_obs * ln(2 pi)"),
                              per_subject = per_subject, total_full = totals, total_no_2pi = totals_no2pi),
              checks = checks)
  write_json_lf(doc, file.path(OUT, paste0(id, ".json")))
  cat(sprintf("%-30s %-18s subjects=%d obs=%d  %.1fs\n", id, family, nsub, nobs, as.numeric(difftime(Sys.time(), t0, units = "secs"))))
  invisible(res)
}

SUBJ <- function(id, time, y, dose = "1") list(id = id, time = time, y = y, dose = dose)

# ================================================================== family a: linear, additive
lin_extra <- function(spec) function(res, subjects) {
  cf <- lapply(subjects, function(s) lin_closed_form(spec, s))
  gap <- function(f) max(vapply(seq_along(res), function(i) asNumeric(abs(cf[[i]]$ofv - f(res[[i]]))), 0))
  list(closed_form_route = "C = Z Omega Z' + s2 I by Cholesky; OFV = n ln 2pi + ln|C| + w'w, w = L^-1 (y - a); eta_hat = Omega Z' C^-1 (y - a)",
       closed_form_vs_linearised_max_abs = D3(gap(function(r) r$forms$linearised)),
       closed_form_vs_fo_max_abs = D3(gap(function(r) r$forms$fo)),
       closed_form_vs_laplace_full_max_abs = D3(gap(function(r) r$forms$laplace_full)),
       closed_form_vs_laplace_expected_max_abs = D3(gap(function(r) r$forms$laplace_expected)),
       closed_form_vs_laplace_no_variance_max_abs = D3(gap(function(r) r$forms$laplace_no_variance)),
       closed_form_eta_vs_mode_max_abs = D3(max(unlist(lapply(seq_along(res), function(i) asNumeric(abs(cf[[i]]$eta - mvec(res[[i]]$eta))))))),
       closed_form_ofv_total_full = S(Reduce(`+`, lapply(cf, function(x) x$ofv))))
}
add_linear <- function(id, desc, theta, re, omega, sigma_a, z_powers, subjects) {
  spec <- make_spec("identity_linear", theta, re, omega, "additive", list(a = sigma_a), z_powers)
  write_case(id, "a_linear", desc, spec, subjects, extra_checks = lin_extra(spec),
             theta_in = theta, omega_in = omega, xi_in = list(a = sigma_a), re_in = re, z_powers = z_powers)
}
TH <- list(theta1 = "1", theta2 = "1")
add_linear("lin_px1", "PX1 of specs/population.md: intercept random effect, 3 subjects, 4 observations each, Omega 0.5, sigma^2 0.25",
  TH, "intercept", list(list("0.5")), "0.5", 0L,
  list(SUBJ("S1", c("1","2","4","8"), c("2.1","2.9","5.2","9.1")), SUBJ("S2", c("1","2","4","8"), c("1.4","2.2","4.1","7.6")),
       SUBJ("S3", c("1","2","4","8"), c("3.0","4.1","5.9","10.4"))))
add_linear("lin_q1_unbalanced", "q = 1, unbalanced: different n_i and times, one subject with a single observation",
  list(theta1 = "0.7", theta2 = "1.3"), "intercept", list(list("0.3")), "0.4", 0L,
  list(SUBJ("S1", c("0.5","1","2.5","6"), c("1.62","2.31","3.88","8.74")), SUBJ("S2", c("2"), c("2.95")),
       SUBJ("S3", c("1","3","4","5","7"), c("1.71","4.97","5.64","7.83","9.52")), SUBJ("S4", c("0.25","8"), c("0.94","11.21"))))
add_linear("lin_q2_diag", "q = 2 (intercept and slope), diagonal Omega, unbalanced, one subject with a single observation",
  list(theta1 = "1.2", theta2 = "0.8"), c("intercept", "slope"), list(list("0.5", "0"), list("0", "0.08")), "0.3", c(0L, 1L),
  list(SUBJ("S1", c("1","2","4","8"), c("2.31","3.02","4.77","7.95")), SUBJ("S2", c("1","3","6"), c("1.64","3.73","5.1")),
       SUBJ("S3", c("2"), c("2.8")), SUBJ("S4", c("0.5","1.5","3","5","9"), c("1.9","2.34","3.65","6.2","8.1"))))
add_linear("lin_q2_block", "q = 2 (intercept and slope), full block Omega (correlation 0.5), unbalanced",
  list(theta1 = "1.2", theta2 = "0.8"), c("intercept", "slope"), list(list("0.5", "0.1"), list("0.1", "0.08")), "0.3", c(0L, 1L),
  list(SUBJ("S1", c("1","2","4","8"), c("2.31","3.02","4.77","7.95")), SUBJ("S2", c("1","3","6"), c("1.64","3.73","5.1")),
       SUBJ("S3", c("2","4"), c("2.8","4.1")), SUBJ("S4", c("0.5","1.5","3","5","9"), c("1.9","2.34","3.65","6.2","8.1"))))
add_linear("lin_m2_small_omega", "m = 2 subjects, Omega = 1e-6 (near the collapse of POP-EST-04)",
  TH, "intercept", list(list("0.000001")), "0.5", 0L,
  list(SUBJ("S1", c("1","2","4"), c("2.12","3.06","4.95")), SUBJ("S2", c("1","2","4"), c("1.85","3.1","5.2"))))
add_linear("lin_large_omega_small_sigma", "Omega = 25, sigma = 0.05: the data dominate the prior",
  TH, "intercept", list(list("25")), "0.05", 0L,
  list(SUBJ("S1", c("1","2","4","8"), c("7.02","8.04","9.95","14.01")), SUBJ("S2", c("1","2","4","8"), c("-3.97","-2.98","-1.04","3.02")),
       SUBJ("S3", c("1","2","4"), c("4.51","5.48","7.52"))))
add_linear("lin_large_sigma", "Omega = 0.01, sigma = 3: the prior dominates the data",
  TH, "intercept", list(list("0.01")), "3", 0L,
  list(SUBJ("S1", c("1","2","4","8"), c("4.1","-0.3","6.7","11.2")), SUBJ("S2", c("1","2","4","8"), c("0.8","5.9","3.4","7.1")),
       SUBJ("S3", c("1","2","4"), c("2.2","6.1","4.4"))))
add_linear("lin_q3_block", "q = 3 (intercept, slope, curvature), full block Omega, 3 subjects with 6 observations",
  list(theta1 = "1", theta2 = "0.5"), c("intercept", "slope", "curvature"),
  list(list("0.6", "0.12", "0.03"), list("0.12", "0.09", "0.015"), list("0.03", "0.015", "0.04")), "0.25", c(0L, 1L, 2L),
  list(SUBJ("S1", c("0.5","1","2","3","5","8"), c("1.42","1.97","3.31","4.62","6.15","9.2")),
       SUBJ("S2", c("0.5","1","2","3","5","8"), c("0.98","1.64","2.72","3.9","5.61","8.77")),
       SUBJ("S3", c("1","2","3","4","6","9"), c("2.2","3.38","4.35","5.9","7.2","10.4"))))

# ================================================================== nonlinear cases (families b, c, d)
IVT <- c("0.5", "1", "2", "4", "8", "12")
ORT <- c("0.5", "1", "2", "4", "8", "16")
PKT <- c("0.25", "1", "2", "4", "10", "24")
TH_IV <- list(v = "50", cl = "5"); TH_OR <- list(v = "50", cl = "5", ka = "1.2")
TH_PK2 <- list(cl = "5", vc = "20", q = "8", vp = "40")
OM1 <- function(x) list(list(x))
OMD2 <- list(list("0.09", "0"), list("0", "0.04"))
OMB2 <- list(list("0.09", "0.03"), list("0.03", "0.04"))

nl_case <- function(id, family, desc, model, theta, re, omega, err, xi, dose, times, m, seed, frozen = "none", marginal = FALSE, subjects = NULL) {
  if (is.null(subjects)) subjects <- simulate_subjects(model, theta, re, omega, err, xi, dose, times, m, seed)
  subjects <- lapply(subjects, function(s) SUBJ(s$id, s$time, s$y, s$dose))
  spec <- make_spec(model, theta, re, omega, err, xi)
  write_case(id, family, desc, spec, subjects, frozen = frozen, do_marginal = marginal,
             theta_in = theta, omega_in = omega, xi_in = xi, re_in = re)
}

# ---- family b: nonlinear, additive error
nl_case("nl_px5", "b_nonlinear", "PX5 of specs/population.md: one subject, three observations, y = 10 exp(-0.3 e^eta t), sigma^2 0.25, Omega 0.4",
  "pk1.iv_bolus", list(v = "10", cl = "3"), "cl", OM1("0.4"), "additive", list(a = "0.5"), "100", NULL, 1L, 0L, marginal = TRUE,
  subjects = list(list(id = "S1", time = c("1", "2", "5"), y = c("6.9", "4.4", "1.6"), dose = "100")))
nl_case("nl_iv_q1", "b_nonlinear", "pk1.iv_bolus, random effect on cl, additive error, 3 subjects, exact marginal by quadrature",
  "pk1.iv_bolus", TH_IV, "cl", OM1("0.25"), "additive", list(a = "0.15"), "100", IVT, 3L, 152101L, marginal = TRUE)
nl_case("nl_iv_q2", "b_nonlinear", "pk1.iv_bolus, random effects on cl and v (diagonal), additive error, 4 subjects",
  "pk1.iv_bolus", TH_IV, c("cl", "v"), OMD2, "additive", list(a = "0.15"), "100", IVT, 4L, 152102L)
nl_case("nl_oral_q1", "b_nonlinear", "pk1.oral_1, random effect on cl, additive error, 3 subjects, exact marginal by quadrature",
  "pk1.oral_1", TH_OR, "cl", OM1("0.16"), "additive", list(a = "0.08"), "100", ORT, 3L, 152103L, marginal = TRUE)
nl_case("nl_oral_q1_large_omega", "b_nonlinear", "pk1.oral_1, random effect on cl with a large Omega (0.8), additive error, exact marginal",
  "pk1.oral_1", TH_OR, "cl", OM1("0.8"), "additive", list(a = "0.08"), "100", ORT, 3L, 152104L, marginal = TRUE)
nl_case("nl_oral_q2_block", "b_nonlinear", "pk1.oral_1, random effects on cl and v with a full block (correlation 0.5), additive error",
  "pk1.oral_1", TH_OR, c("cl", "v"), OMB2, "additive", list(a = "0.08"), "100", ORT, 3L, 152105L)
nl_case("nl_pk2_q1", "b_nonlinear", "pk2.iv_bolus (clearance set), random effect on cl, additive error, exact marginal",
  "pk2.iv_bolus", TH_PK2, "cl", OM1("0.16"), "additive", list(a = "0.1"), "100", PKT, 3L, 152106L, marginal = TRUE)
nl_case("nl_pk2_q2", "b_nonlinear", "pk2.iv_bolus (clearance set), random effects on cl and vc (diagonal), additive error",
  "pk2.iv_bolus", TH_PK2, c("cl", "vc"), OMD2, "additive", list(a = "0.1"), "100", PKT, 3L, 152107L)

# ---- family c: sigma frozen
PX2_SUBJECTS <- list(
  SUBJ("S1", IVT, c("2.44", "2.39", "1.98", "1.38", "0.600", "0.416"), "100"), SUBJ("S2", IVT, c("1.59", "1.72", "1.62", "1.33", "0.789", "0.417"), "100"),
  SUBJ("S3", IVT, c("1.40", "1.68", "1.14", "1.15", "0.732", "0.453"), "100"), SUBJ("S4", IVT, c("2.23", "2.00", "1.57", "1.47", "0.869", "0.559"), "100"))
nl_case("frz_px2_mode", "c_frozen", "PX2 data and parameters (proportional, q = 2), sigma frozen at its value at the mode (fixed point)",
  "pk1.iv_bolus", TH_IV, c("cl", "v"), OMD2, "proportional", list(b = "0.1"), "100", NULL, 4L, 0L, frozen = "mode", subjects = PX2_SUBJECTS)
nl_case("frz_px2_pred0", "c_frozen", "PX2 data and parameters (proportional, q = 2), sigma frozen at the population prediction yhat(0)",
  "pk1.iv_bolus", TH_IV, c("cl", "v"), OMD2, "proportional", list(b = "0.1"), "100", NULL, 4L, 0L, frozen = "pred0", subjects = PX2_SUBJECTS)
nl_case("frz_iv_combined_mode", "c_frozen", "pk1.iv_bolus, q = 2, combined error, sigma frozen at the mode",
  "pk1.iv_bolus", TH_IV, c("cl", "v"), OMD2, "combined", list(a = "0.05", b = "0.1"), "100", IVT, 3L, 152201L, frozen = "mode")
nl_case("frz_oral_prop_mode", "c_frozen", "pk1.oral_1, q = 1, proportional error, sigma frozen at the mode",
  "pk1.oral_1", TH_OR, "cl", OM1("0.16"), "proportional", list(b = "0.12"), "100", ORT, 3L, 152202L, frozen = "mode")
nl_case("frz_oral_combined_pred0", "c_frozen", "pk1.oral_1, q = 2 block, combined error, sigma frozen at the population prediction",
  "pk1.oral_1", TH_OR, c("cl", "v"), OMB2, "combined", list(a = "0.04", b = "0.1"), "100", ORT, 3L, 152203L, frozen = "pred0")
nl_case("frz_pk2_combsd_mode", "c_frozen", "pk2.iv_bolus, q = 2, combined_sd error, sigma frozen at the mode",
  "pk2.iv_bolus", TH_PK2, c("cl", "vc"), OMD2, "combined_sd", list(a = "0.03", b = "0.08"), "100", PKT, 3L, 152204L, frozen = "mode")

# ---- family d: interaction (sigma depends on eta)
nl_case("int_px2", "d_interaction", "PX2 of specs/population.md: pk1.iv_bolus, q = 2, proportional error, four subjects",
  "pk1.iv_bolus", TH_IV, c("cl", "v"), OMD2, "proportional", list(b = "0.1"), "100", NULL, 4L, 0L, subjects = PX2_SUBJECTS)
nl_case("int_iv_prop_q1", "d_interaction", "pk1.iv_bolus, random effect on cl, proportional error, exact marginal",
  "pk1.iv_bolus", TH_IV, "cl", OM1("0.25"), "proportional", list(b = "0.12"), "100", IVT, 3L, 152301L, marginal = TRUE)
nl_case("int_oral_comb_q1", "d_interaction", "pk1.oral_1, random effect on cl, combined error, exact marginal",
  "pk1.oral_1", TH_OR, "cl", OM1("0.16"), "combined", list(a = "0.04", b = "0.1"), "100", ORT, 3L, 152302L, marginal = TRUE)
nl_case("int_pk2_combsd_q1", "d_interaction", "pk2.iv_bolus, random effect on cl, combined_sd error, exact marginal",
  "pk2.iv_bolus", TH_PK2, "cl", OM1("0.16"), "combined_sd", list(a = "0.03", b = "0.08"), "100", PKT, 3L, 152303L, marginal = TRUE)
nl_case("int_iv_comb_q1_large", "d_interaction", "pk1.iv_bolus, random effect on cl with Omega 0.5, combined error, exact marginal",
  "pk1.iv_bolus", TH_IV, "cl", OM1("0.5"), "combined", list(a = "0.05", b = "0.15"), "100", IVT, 3L, 152304L, marginal = TRUE)
nl_case("int_oral_prop_q2_block", "d_interaction", "pk1.oral_1, q = 2 block, proportional error",
  "pk1.oral_1", TH_OR, c("cl", "v"), OMB2, "proportional", list(b = "0.12"), "100", ORT, 3L, 152305L)
nl_case("int_pk2_prop_q2", "d_interaction", "pk2.iv_bolus, q = 2 diagonal, proportional error",
  "pk2.iv_bolus", TH_PK2, c("cl", "vc"), OMD2, "proportional", list(b = "0.1"), "100", PKT, 3L, 152306L)

# ------------------------------------------------------------------ options file
write_json_lf(list(schema = 1L, kind = "pop_exact_options", generated_by = "oracle/scripts/pop_exact.R",
                   versions = versions_list(), precision_bits = BITS, digits = SIG,
                   quadrature = list(rule = "trapezoidal", cut_sd = 13L, step_rule = "h = min(0.6 s_post, sd/2), compared with h/2"),
                   seeds = "data of the nonlinear cases is simulated with set.seed(seed) in Mersenne-Twister/Inversion/Rejection, rounded to 3 significant digits; the seed is in the script",
                   note = "inputs are decimal strings; expected values are 50-significant-digit decimal strings rounded from 256 bits"),
              file.path(OUT, "options.json"))
cat("done\n")
