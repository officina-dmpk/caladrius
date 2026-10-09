# Caladrius two-compartment model oracle, part 2 (task T-032): derivative grids and error cases.
# Sourced by models_2c_closed_form.R (it uses its functions and its paths); not run on its own.

# ---------------------------------------------------------------- derivative grids (OM-10)
# dC/dtheta by a central difference in 256-bit arithmetic with the relative step 1e-25: the
# truncation error is of order 1e-50 and the rounding error 1e-77 / 1e-25 times the conditioning,
# so the expected values are exact to double precision. theta is every parameter of the set the case
# is given in (clearance: cl, vc, q, vp; micro: k10, k12, k21, vc; macro: a, b, alpha, beta, the
# dose being fixed) and every input parameter (ka, tlag, dur).
#
# Conditioning (the only filtering): C = (1/V) [ w Phi(alpha) + (1 - w) Phi(beta) ] (MOD-2C-17), so
# dC/dtheta is a sum over the internal parameters (V, alpha, beta, w) of dC/dpsi_j * dpsi_j/dtheta,
# each factor again a 256-bit central difference, and for ka, tlag, dur of two terms. The condition
# number of that sum, kappa = (sum of the absolute terms) / |sum|, says how many digits a double
# precision evaluation of the closed form (the chain rule of MOD-2C-18/19) must lose; an entry with
# kappa > KAPPA_MAX is not written (it is a zero crossing of the derivative, where no relative
# tolerance can be asked of any double precision code). The sum of the terms must reproduce the
# direct central difference to 1e-40 (checked here: it tests the decomposition).
KAPPA_MAX <- 1e3
STEP <- mp(10)^-25

conc_eval <- function(model, set, p, dose, extra, t) {
  q <- derive_q(set, p, dose, extra)
  conc_at(segments(model, q), t, model == "pk2.iv_bolus")
}
cdiff <- function(f, theta) {
  h <- abs(theta) * STEP
  (f(theta + h) - f(theta - h)) / (2 * h)
}
psi_of <- function(set, p, dose, extra) {
  q <- derive_q(set, p, dose, extra)
  list(V = q$vc, alpha = q$alpha, beta = q$beta, w = q$wa)
}

deriv_times <- function(model, q, extra) {
  al <- num(q$alpha); be <- num(q$beta)
  ta <- 1 / al; tb <- 1 / be
  ts <- c(ta * c(0.1, 0.5, 1, 2, 4), tb * c(0.25, 0.5, 1, 2, 4, 8, 16, 40))
  if (!is.null(extra$ka)) ts <- c(ts, c(0.1, 1, 5) / extra$ka)
  tl <- if (is.null(extra$tlag)) 0 else extra$tlag
  if (!is.null(extra$tlag)) ts <- c(ts, tl / 2, tl + ta * c(0.5, 2), tl + tb * c(1, 4))
  if (!is.null(extra$dur)) ts <- c(ts, tl + extra$dur * c(0.5, 1.5, 2))
  ts <- signif(ts, 6)
  # no time on a kink (the start of the input and the end of a zero-order input)
  kinks <- c(tl, if (!is.null(extra$dur)) tl + extra$dur)
  ts <- ts[vapply(ts, function(t) all(abs(t - kinks) > 1e-9 * max(1, abs(t))), TRUE)]
  sort(unique(c(-1, 0, ts)))
}

write_deriv_case <- function(name, model, set, p, dose, extra, note) {
  bolus <- model == "pk2.iv_bolus"
  q <- derive_q(set, p, dose, extra)
  times <- deriv_times(model, q, extra)
  pn <- names(p)
  en <- names(extra)
  psi0 <- psi_of(set, p, dose, extra)
  psi_names <- c("V", "alpha", "beta", "w")
  # d psi_j / d theta for the set parameters (independent of t)
  psi_theta <- lapply(pn, function(nm) {
    vapply(psi_names, function(j) {
      num(cdiff(function(x) { p2 <- p; p2[[nm]] <- x; psi_of(set, p2, dose, extra)[[j]] }, mp(p[[nm]])))
    }, 0)
  })
  names(psi_theta) <- pn
  # the psi-set parameters with w = 1 and w = 0 (to isolate Phi(alpha) and Phi(beta) for ka, tlag, dur)
  psi_w <- function(w) { ps <- psi0; ps$w <- mp(w); ps }
  rows <- list()
  omitted <- 0L
  omitted_zero <- 0L
  kappa_max_seen <- 0
  for (t in times) {
    ctime <- abs(num(conc_eval(model, set, p, dose, extra, t)))
    # d C / d psi_j at (psi0, extra)
    Cpsi <- vapply(psi_names, function(j) {
      num(cdiff(function(x) { ps <- psi0; ps[[j]] <- x; conc_eval(model, "psi", ps, dose, extra, t) }, psi0[[j]]))
    }, 0)
    for (nm in c(pn, en)) {
      if (nm %in% pn) {
        d <- cdiff(function(x) { p2 <- p; p2[[nm]] <- x; conc_eval(model, set, p2, dose, extra, t) }, mp(p[[nm]]))
        terms <- Cpsi * psi_theta[[nm]]
        total <- sum(abs(terms)); dsum <- sum(terms)
      } else {
        d <- cdiff(function(x) { e2 <- extra; e2[[nm]] <- x; conc_eval(model, set, p, dose, e2, t) }, mp(extra[[nm]]))
        w <- psi0$w
        d1 <- cdiff(function(x) { e2 <- extra; e2[[nm]] <- x; conc_eval(model, "psi", psi_w(1), dose, e2, t) }, mp(extra[[nm]]))
        d0 <- cdiff(function(x) { e2 <- extra; e2[[nm]] <- x; conc_eval(model, "psi", psi_w(0), dose, e2, t) }, mp(extra[[nm]]))
        terms <- c(num(w * d1), num((1 - w) * d0))
        total <- sum(abs(terms)); dsum <- num(w * d1 + (1 - w) * d0)
      }
      dn <- num(d)
      if (ctime == 0) {
        # before the dose and before the lag, and at the start of a non-bolus input, C = 0 whatever
        # the parameters: every derivative is exactly 0 and the engine must return exactly 0
        check(dn == 0 && total == 0, name, ": a derivative at a time with C = 0: d", nm, " at t = ", t)
        rows[[length(rows) + 1L]] <- list(t = t, nm = nm, v = 0)
        next
      }
      # A derivative that is zero by construction (for example dC/dcl at t = 0 of the bolus, where
      # C = D / vc) comes out of the 256-bit differences as rounding noise of order 1e-52, not as 0;
      # the true value is 0 and it is written as an exact 0 (MODEL_DERIVATIVES has no absolute part).
      noise <- 1e-30 * ctime / (if (nm %in% pn) abs(num(p[[nm]])) else 1)
      if (abs(dn) <= noise && total <= noise) {
        omitted_zero <- omitted_zero + 1L
        rows[[length(rows) + 1L]] <- list(t = t, nm = nm, v = 0)
        next
      }
      # the decomposition must reproduce the direct central difference (double check of the code path)
      check(abs(dn - dsum) <= 1e-9 * (abs(dn) + total * 1e-6) + 1e-300, name, ": decomposition of d", nm,
            " at t = ", t, ": ", dn, " against ", dsum)
      kappa <- if (dn == 0) Inf else total / abs(dn)
      if (kappa > KAPPA_MAX) { omitted <- omitted + 1L; next }
      kappa_max_seen <- max(kappa_max_seen, kappa)
      rows[[length(rows) + 1L]] <- list(t = t, nm = nm, v = dn)
    }
  }
  df <- data.frame(subject = paste0("t=", num17(vapply(rows, function(r) r$t, 0))),
                   parameter = paste0("d_", vapply(rows, function(r) r$nm, "")),
                   value = vapply(rows, function(r) r$v, 0))
  df <- df[order(match(df$subject, paste0("t=", num17(times)))), ]
  base <- paste0("model_pk2_deriv_", name)
  write_lines_lf(c("subject,parameter,value", sprintf("%s,%s,%s", df$subject, df$parameter, num17(df$value))),
                 file.path(exp_dir, paste0(base, ".csv")))
  meta <- list(
    schema = 1L, kind = "model_derivatives", case = base,
    generated_by = "oracle/scripts/models_2c_closed_form.R",
    model = model, parameterisation = set, dose = dose,
    textbook_double_ok = textbook_flag(q, extra),
    parameters = c(p, extra),
    times = arr(times),
    units = list(dose = "mg", time = "h", concentration = "mg/L"),
    note = note,
    quantities = list(
      derivative = "d_<name> at a time is the partial derivative of the concentration with respect to the parameter <name> of this case (the dose fixed; for the macro set vc = D / (a + b) moves with a and b); 0 before the dose and before the lag",
      omitted = "an entry whose condition number (sum of the absolute terms of the chain rule through V, alpha, beta, w, or of the two one-compartment terms for ka, tlag, dur, divided by the absolute sum) exceeds 1e3 is not written: no double precision closed form can be asked for 1e-12 relative accuracy near a zero of the derivative"
    ),
    method = list(
      derivatives = "central differences in 256-bit arithmetic (Rmpfr), relative step 1e-25, of the independent explicit form of the concentration; rounded once to double",
      kappa_max = KAPPA_MAX,
      n_omitted_ill_conditioned = omitted,
      n_structural_zero_written_as_zero = omitted_zero,
      kappa_max_kept = kappa_max_seen
    ),
    versions = versions,
    n_values = nrow(df),
    n_times = length(times)
  )
  write_lines_lf(jenc(meta), file.path(exp_dir, paste0(base, ".options.json")))
  cat(sprintf("%-52s %3d times %4d values (%d omitted)\n", base, length(times), nrow(df), omitted))
  nrow(df)
}

deriv_cases <- list()
add_deriv <- function(id, name, point, set, extra = list(), note = "") {
  pt <- points[[point]]
  deriv_cases[[length(deriv_cases) + 1L]] <<- list(
    name = paste0(id, "_", name), model = paste0("pk2.", id), set = set, p = point_in(point, set),
    dose = pt$dose, extra = extra, note = note)
}
ka_of <- c(base = 2, distribution = 3, small_exchange = 0.5, ab_ratio_1e3 = 5, near_degenerate_1e6 = 0.2)
tlag_of <- c(base = 0.5, distribution = 0.25, small_exchange = 2, ab_ratio_1e3 = 3, near_degenerate_1e6 = 0.5)
dur_of <- inf_dur
extra_for <- function(id, pt) {
  switch(id,
    iv_bolus = list(),
    iv_infusion = list(dur = dur_of[[pt]]),
    oral_1 = list(ka = ka_of[[pt]]),
    oral_1_lag = list(ka = ka_of[[pt]], tlag = tlag_of[[pt]]),
    oral_0 = list(dur = dur_of[[pt]]),
    oral_0_lag = list(dur = dur_of[[pt]], tlag = tlag_of[[pt]]))
}
for (id in c("iv_bolus", "iv_infusion", "oral_1", "oral_1_lag", "oral_0", "oral_0_lag")) {
  for (pt in c("base", "distribution", "small_exchange", "ab_ratio_1e3", "near_degenerate_1e6")) {
    add_deriv(id, pt, pt, "clearance", extra_for(id, pt), paste0("derivatives, point ", pt, ", clearance set"))
  }
  for (pt in c("base", "distribution", "near_degenerate_1e6")) {
    add_deriv(id, paste0(pt, "_micro"), pt, "micro", extra_for(id, pt), paste0("derivatives, point ", pt, ", micro set"))
  }
  for (pt in c("base", "distribution")) {
    add_deriv(id, paste0(pt, "_macro"), pt, "macro", extra_for(id, pt), paste0("derivatives, point ", pt, ", macro set"))
  }
}
add_deriv("oral_1", "ka_alpha_plus_1e6", "base", "clearance", list(ka = 1 + 1e-6), "ka = alpha (1 + 1e-6)")
add_deriv("oral_1", "ka_between", "base", "clearance", list(ka = 0.4), "beta < ka < alpha")
add_deriv("oral_1", "ka_below_beta", "base", "clearance", list(ka = 0.05), "ka < beta")
add_deriv("oral_1", "ka_eq_alpha_macro", "base", "macro", list(ka = point_in("base", "macro")$alpha), "ka = alpha exactly, macro set")

total_deriv <- 0L
for (cs in deriv_cases) {
  total_deriv <- total_deriv + write_deriv_case(cs$name, cs$model, cs$set, cs$p, cs$dose, cs$extra, cs$note)
}
cat(sprintf("derivative cases: %d, %d values\n", length(deriv_cases), total_deriv))

# ---------------------------------------------------------------- error cases (OM-07)
# What the engine must refuse, as expected "not available" rows (an empty value in the table) with
# the reason, the rule, and the words the readable message must contain (lower case; each inner
# list is a set of alternatives of which one must appear). Non-finite numbers are written as the
# strings "NaN", "Infinity", "-Infinity" because JSON has none.
base_cl <- list(cl = 2, vc = 10, q = 4, vp = 8)
base_micro <- list(k10 = 0.2, k12 = 0.4, k21 = 0.5, vc = 10)
base_macro <- list(a = 50 / 9, b = 40 / 9, alpha = 1, beta = 0.1)
errors <- list()
err <- function(id, group, spec, model, dose, params, contains, reason, times = 1) {
  errors[[length(errors) + 1L]] <<- list(
    id = id, group = group, spec = spec, model = model, dose = dose, parameters = params,
    times = arr(times), message_contains = lapply(contains, arr), reason = reason)
}
with_p <- function(p, ...) { mod <- do.call(c, list(...)); for (n in names(mod)) p[[n]] <- mod[[n]]; p }
without <- function(p, ...) p[setdiff(names(p), c(...))]

# domain of the clearance set (MOD-2C-05, MOD-GEN-04)
for (nm in names(base_cl)) {
  for (bad in list(list("zero", 0), list("negative", -1), list("nan", "NaN"), list("infinite", "Infinity"))) {
    err(paste0("clearance_", nm, "_", bad[[1]]), "domain", "MOD-2C-05, MOD-GEN-04", "pk2.iv_bolus", 100,
        with_p(base_cl, setNames(list(bad[[2]]), nm)), list(nm),
        paste0("`", nm, "` must be finite and > 0"))
  }
}
# q = 0 or vp = 0 is a one-compartment model (the message points to pk1)
for (nm in c("q", "vp")) {
  for (m in c("pk2.iv_bolus", "pk2.iv_infusion", "pk2.oral_1")) {
    extra_p <- switch(m, pk2.iv_bolus = list(), pk2.iv_infusion = list(dur = 2), pk2.oral_1 = list(ka = 1))
    p <- c(with_p(base_cl, setNames(list(0), nm)), extra_p)
    err(paste0("one_compartment_", nm, "_zero_", sub("pk2.", "", m, fixed = TRUE)), "one_compartment",
        "MOD-2C-05 item 1", m, 100, p, list(nm, "pk1"),
        paste0("`", nm, "` = 0 makes this a one-compartment model; the message names the parameter and says to use pk1"))
  }
}
err("micro_k12_zero", "one_compartment", "MOD-2C-05 item 1", "pk2.iv_bolus", 100, with_p(base_micro, k12 = 0),
    list("k12", "pk1"), "k12 = 0 is a one-compartment model")
for (nm in c("k10", "k12", "k21", "vc")) {
  err(paste0("micro_", nm, "_negative"), "domain", "MOD-2C-05", "pk2.iv_bolus", 100,
      with_p(base_micro, setNames(list(-0.5), nm)), list(nm), paste0("`", nm, "` must be > 0"))
}
err("micro_k10_zero", "domain", "MOD-2C-05", "pk2.iv_bolus", 100, with_p(base_micro, k10 = 0), list("k10"), "`k10` must be > 0")
err("micro_k21_nan", "domain", "MOD-2C-05, MOD-GEN-04", "pk2.iv_bolus", 100, with_p(base_micro, k21 = "NaN"), list("k21"), "`k21` must be finite")
# input parameters
err("ka_zero", "domain", "MOD-GEN-04", "pk2.oral_1", 100, c(base_cl, list(ka = 0)), list("ka"), "`ka` must be > 0")
err("ka_negative", "domain", "MOD-GEN-04", "pk2.oral_1", 100, c(base_cl, list(ka = -1)), list("ka"), "`ka` must be > 0")
err("ka_nan", "domain", "MOD-GEN-04", "pk2.oral_1_lag", 100, c(base_cl, list(ka = "NaN", tlag = 0.5)), list("ka"), "`ka` must be finite")
err("ka_infinite", "domain", "MOD-GEN-04", "pk2.oral_1", 100, c(base_cl, list(ka = "Infinity")), list("ka"), "`ka` must be finite")
err("tlag_negative", "domain", "MOD-GEN-04", "pk2.oral_1_lag", 100, c(base_cl, list(ka = 1, tlag = -0.5)), list("tlag"), "`tlag` must be >= 0")
err("tlag_nan", "domain", "MOD-GEN-04", "pk2.oral_0_lag", 100, c(base_cl, list(dur = 2, tlag = "NaN")), list("tlag"), "`tlag` must be finite")
err("dur_zero", "domain", "MOD-GEN-04", "pk2.iv_infusion", 100, c(base_cl, list(dur = 0)), list("dur"), "`dur` must be > 0")
err("dur_negative", "domain", "MOD-GEN-04", "pk2.oral_0", 100, c(base_cl, list(dur = -1)), list("dur"), "`dur` must be > 0")
err("dur_infinite", "domain", "MOD-GEN-04", "pk2.oral_0", 100, c(base_cl, list(dur = "Infinity")), list("dur"), "`dur` must be finite")
err("dose_negative", "domain", "MOD-GEN-04", "pk2.iv_bolus", -1, base_cl, list("dose"), "the dose must be >= 0")
err("dose_nan", "domain", "MOD-GEN-04", "pk2.iv_bolus", "NaN", base_cl, list("dose"), "the dose must be finite")
err("dose_infinite", "domain", "MOD-GEN-04", "pk2.iv_bolus", "Infinity", base_cl, list("dose"), "the dose must be finite")
# macro set (MOD-2C-05 item 2)
err("macro_alpha_equals_beta", "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, alpha = 0.1), list("alpha", "beta"), "alpha must be larger than beta")
err("macro_alpha_below_beta", "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, alpha = 0.1, beta = 1), list("alpha", "beta"), "the larger exponent must be called alpha; the engine does not sort")
for (nm in c("a", "b")) {
  err(paste0("macro_", nm, "_zero"), "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, setNames(list(0), nm)), list(nm), paste0("`", nm, "` must be > 0"))
  err(paste0("macro_", nm, "_negative"), "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, setNames(list(-1), nm)), list(nm), paste0("`", nm, "` must be > 0"))
}
err("macro_beta_zero", "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, beta = 0), list("beta"), "`beta` must be > 0")
err("macro_beta_negative", "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, beta = -0.1), list("beta"), "`beta` must be > 0")
err("macro_alpha_nan", "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 100, with_p(base_macro, alpha = "NaN"), list("alpha"), "`alpha` must be finite")
err("macro_dose_zero", "macro", "MOD-2C-05 item 2", "pk2.iv_bolus", 0, base_macro, list("dose"), "vc = D / (a + b) needs a dose > 0")
err("macro_dose_zero_oral", "macro", "MOD-2C-05 item 2", "pk2.oral_1", 0, c(base_macro, list(ka = 2)), list("dose"), "vc = D / (a + b) needs a dose > 0")
# sets and names (MOD-2C-02, MOD-2C-05 item 4)
err("sets_clearance_plus_k10", "sets", "MOD-2C-02", "pk2.iv_bolus", 100, c(base_cl, list(k10 = 0.2)), list(c("k10", "cl")), "a mixture of two sets names the offending parameter")
err("sets_clearance_plus_micro", "sets", "MOD-2C-02", "pk2.iv_bolus", 100, c(base_cl, base_micro[c("k10", "k12", "k21")]), list(c("k10", "k12", "k21", "cl")), "two complete sets")
err("sets_clearance_plus_alpha", "sets", "MOD-2C-02", "pk2.iv_bolus", 100, c(base_cl, list(alpha = 1)), list(c("alpha", "cl")), "a macro parameter next to the clearance set")
err("sets_micro_plus_a", "sets", "MOD-2C-02", "pk2.iv_bolus", 100, c(base_micro, list(a = 5)), list(c("a", "k10", "k12", "k21")), "a macro parameter next to the micro set")
err("sets_missing_vp", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, without(base_cl, "vp"), list("vp"), "an incomplete clearance set names the missing parameter")
err("sets_missing_cl", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, without(base_cl, "cl"), list("cl"), "an incomplete clearance set names the missing parameter")
err("sets_missing_q", "sets", "MOD-2C-05 item 4", "pk2.oral_1", 100, c(without(base_cl, "q"), list(ka = 1)), list("q"), "an incomplete clearance set names the missing parameter")
err("sets_missing_vc_micro", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, without(base_micro, "vc"), list("vc"), "an incomplete micro set")
err("sets_missing_beta_macro", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, without(base_macro, "beta"), list("beta"), "an incomplete macro set")
err("sets_none", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, list(), list(c("cl", "vc", "k10", "alpha")), "no parameter at all")
err("sets_ka_on_bolus", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, c(base_cl, list(ka = 1)), list("ka"), "`ka` does not belong to this model")
err("sets_dur_on_oral_1", "sets", "MOD-2C-05 item 4", "pk2.oral_1", 100, c(base_cl, list(ka = 1, dur = 2)), list("dur"), "`dur` does not belong to this model")
err("sets_tlag_on_oral_1", "sets", "MOD-2C-05 item 4", "pk2.oral_1", 100, c(base_cl, list(ka = 1, tlag = 0.5)), list("tlag"), "`tlag` does not belong to the model without lag")
err("sets_ka_on_oral_0", "sets", "MOD-2C-05 item 4", "pk2.oral_0", 100, c(base_cl, list(dur = 2, ka = 1)), list("ka"), "`ka` does not belong to this model")
err("sets_tlag_on_infusion", "sets", "MOD-2C-05 item 4", "pk2.iv_infusion", 100, c(base_cl, list(dur = 2, tlag = 0.5)), list("tlag"), "an intravenous model has no lag")
err("sets_missing_ka", "sets", "MOD-2C-05 item 4", "pk2.oral_1", 100, base_cl, list("ka"), "`ka` is required")
err("sets_missing_dur", "sets", "MOD-2C-05 item 4", "pk2.iv_infusion", 100, base_cl, list("dur"), "`dur` is required")
err("sets_missing_tlag", "sets", "MOD-2C-05 item 4", "pk2.oral_1_lag", 100, c(base_cl, list(ka = 1)), list("tlag"), "`tlag` is required")
err("sets_one_compartment_v", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, c(without(base_cl, "vc"), list(v = 10)), list(c("v", "vc")), "the one-compartment name `v` is not a two-compartment parameter")
err("sets_one_compartment_k", "sets", "MOD-2C-05 item 4", "pk2.iv_bolus", 100, c(base_cl, list(k = 0.2)), list("k"), "the one-compartment name `k` is not a two-compartment parameter")
err("sets_unknown_name", "sets", "MOD-GEN-04", "pk2.iv_bolus", 100, c(base_cl, list(volume = 3)), list("volume"), "an unknown name is refused by name")
# numerics (MOD-2C-05 item 3, MOD-2C-22)
err("numeric_overflow_c0", "numeric", "MOD-2C-22", "pk2.iv_bolus", 1e300, with_p(base_cl, vc = 1e-300, cl = 2e-300, q = 4e-300, vp = 8e-300), list(c("overflow", "finite")), "D / vc overflows: an Overflow error, never an infinity")
err("numeric_overflow_derived_rate", "numeric", "MOD-2C-05 item 3", "pk2.iv_bolus", 100, with_p(base_cl, q = 1e300, vp = 1e-300), list(c("q", "vp", "k21", "overflow", "finite")), "q / vp is not finite")
err("numeric_overflow_macro", "numeric", "MOD-2C-22", "pk2.iv_bolus", 100, with_p(base_macro, a = 1e308, b = 1e308), list(c("a", "b", "vc", "overflow", "finite")), "a + b overflows")
err("numeric_degenerate_exponents", "numeric", "MOD-2C-05 item 3", "pk2.iv_bolus", 100, list(k10 = 1, k12 = 1e-200, k21 = 1e-200, vc = 1), list(), "k12 * k21 underflows to 0: DegenerateExponents")
# times (MOD-GEN-04)
err("times_nan", "times", "MOD-GEN-04", "pk2.iv_bolus", 100, base_cl, list(c("time", "nan", "finite")), "a NaN time is refused", times = list(1, "NaN"))
err("times_infinite", "times", "MOD-GEN-04", "pk2.iv_bolus", 100, base_cl, list(c("time", "infinite", "finite")), "an infinite time is refused", times = list("Infinity"))
err("times_negative_infinite", "times", "MOD-GEN-04", "pk2.iv_bolus", 100, base_cl, list(c("time", "infinite", "finite")), "an infinite time is refused", times = list("-Infinity"))

ids <- vapply(errors, function(e) e$id, "")
check(!any(duplicated(ids)), "duplicate error ids")
# every parameter list must be a named list (an empty one is an object)
errors <- lapply(errors, function(e) {
  if (length(e$parameters) == 0L) e$parameters <- structure(list(), names = character(0))
  e
})
base <- "model_pk2_errors"
write_lines_lf(c("subject,parameter,value", sprintf("%s,result,", ids)),
               file.path(exp_dir, paste0(base, ".csv")))
write_lines_lf(jenc(list(
  schema = 1L, kind = "model_errors", case = base,
  generated_by = "oracle/scripts/models_2c_closed_form.R",
  note = "inputs the two-compartment engine must refuse; the expected table has one not-available row (empty value) per case, the reason is in `reason`, the rule in `spec`, and the readable message must contain, for each inner list of `message_contains`, at least one of its words (lower case)",
  groups = list("domain", "one_compartment", "macro", "sets", "numeric", "times"),
  cases = errors,
  versions = versions,
  n_values = length(errors)
)), file.path(exp_dir, paste0(base, ".options.json")))
cat(sprintf("error cases: %d\n", length(errors)))

# ---------------------------------------------------------------- the list of cases for the engine test
# crates/caladrius-models/tests/oracle_models_2c.rs includes this file: the case lists, the guard's
# constants and the invocations of its two macros. Regenerated with the oracle.
case_list <- function(names) paste0("    \"", names, "\",", collapse = "\n")
macro_list <- function(names) paste0("    ", names, ",", collapse = "\n")
vnames <- vapply(value_cases, function(c) c$name, "")
dnames <- vapply(deriv_cases, function(c) c$name, "")
write_lines_lf(c(
  "// Generated by oracle/scripts/models_2c_closed_form.R; do not edit by hand.",
  "const VALUE_CASES: &[&str] = &[", case_list(vnames), "];",
  "const DERIVATIVE_CASES: &[&str] = &[", case_list(dnames), "];",
  "value_tests! {", macro_list(vnames), "}",
  "derivative_tests! {", macro_list(dnames), "}"),
  file.path(root, "crates", "caladrius-models", "tests", "oracle_models_2c.cases"))
cat(sprintf("test list: %d value cases, %d derivative cases\n", length(vnames), length(dnames)))
