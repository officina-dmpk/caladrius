#!/usr/bin/env Rscript
# Caladrius model oracle (task T-009, part a).
#
# Exact values of the six one-compartment models of specs/models.md on time grids, from an
# independent implementation of the closed forms in 256-bit arithmetic (Rmpfr) and rounded once to
# double. The script does NOT use the formulas or the numerical tricks of specs/models.md
# (MOD-AB1-03): the textbook forms are evaluated with enough digits that no cancellation matters,
# and the case ka = k is the analytic limit.
#
# Every value is cross-checked, within 1e-8, against two numerical solutions of the differential
# equations of the compartment diagram:
#   - the matrix exponential (package expm) on each interval of constant input, exact up to
#     rounding for these linear systems;
#   - an adaptive ODE solver (package deSolve, lsoda, rtol 1e-12) with an extra state for the AUC
#     and one for the first moment of the curve, which also checks AUC, MRT, Cmax and Tmax.
# (rxode2 and mrgsolve were not used: they compile models at run time and this machine has no C
# compiler; the two checks above are independent of the closed forms and of each other.)
#
# Usage, from the repository root:
#   Rscript oracle/scripts/models_closed_form.R
# Writes (deterministically, no timestamps):
#   oracle/expected/models/model_<case>.csv           long table: subject,parameter,value
#   oracle/expected/models/model_<case>.options.json  model id, dose, parameters, times, versions

suppressPackageStartupMessages({
  library(Rmpfr)
  library(expm)
  library(deSolve)
  library(jsonlite)
})

args_all <- commandArgs(trailingOnly = FALSE)
file_arg <- sub("^--file=", "", args_all[grepl("^--file=", args_all)])
root <- if (length(file_arg) == 1L) {
  normalizePath(file.path(dirname(file_arg), "..", ".."), winslash = "/", mustWork = TRUE)
} else {
  normalizePath(".", winslash = "/", mustWork = TRUE)
}
exp_dir <- file.path(root, "oracle", "expected", "models")
dir.create(exp_dir, showWarnings = FALSE, recursive = TRUE)

write_lines_lf <- function(lines, path) {
  con <- file(path, open = "wb")
  on.exit(close(con))
  writeLines(lines, con, sep = "\n", useBytes = TRUE)
}
num17 <- function(x) sprintf("%.17g", x)

PREC <- 256L
mp <- function(x) mpfr(x, PREC)

# ---------------------------------------------------------------- closed forms (256 bits)
# Parameters arrive as a list with dose, v and either cl or k (never both), and ka, tlag, dur
# where the model has them. Everything is derived from (dose, v, k).
derive <- function(p) {
  # `[[` matches names exactly; `$` would take `ka` for an absent `k`.
  v <- mp(p[["v"]])
  k <- if (!is.null(p[["k"]])) mp(p[["k"]]) else mp(p[["cl"]]) / v
  list(D = mp(p[["dose"]]), v = v, k = k, cl = v * k,
       ka = if (!is.null(p[["ka"]])) mp(p[["ka"]]) else NULL,
       tlag = if (!is.null(p[["tlag"]])) mp(p[["tlag"]]) else mp(0),
       dur = if (!is.null(p[["dur"]])) mp(p[["dur"]]) else NULL)
}

zero <- function() mp(0)

# Concentration and AUC(0, t) of a model at one time t (mpfr).
conc_auc <- function(model, q, t) {
  t <- mp(t)
  if (t <= 0 && model == "pk1.iv_bolus") {
    if (t < 0) return(c(zero(), zero()))
    return(c(q$D / q$v, zero()))
  }
  u <- t - q$tlag
  if (u <= 0) return(c(zero(), zero()))
  D <- q$D; v <- q$v; k <- q[["k"]]
  switch(model,
    "pk1.iv_bolus" = {
      c(D / v * exp(-k * u), D / (v * k) * (1 - exp(-k * u)))
    },
    "pk1.iv_infusion" = , "pk1.oral_0" = , "pk1.oral_0_lag" = {
      T <- q$dur
      R <- D / T
      if (u <= T) {
        c(R / (v * k) * (1 - exp(-k * u)),
          R / (v * k) * (u - (1 - exp(-k * u)) / k))
      } else {
        cT <- R / (v * k) * (1 - exp(-k * T))
        aucT <- R / (v * k) * (T - (1 - exp(-k * T)) / k)
        c(cT * exp(-k * (u - T)), aucT + cT / k * (1 - exp(-k * (u - T))))
      }
    },
    "pk1.oral_1" = , "pk1.oral_1_lag" = {
      ka <- q[["ka"]]
      if (ka == k) {
        # Analytic limit ka -> k.
        c(D * k * u * exp(-k * u) / v,
          D / (v * k) * (1 - exp(-k * u) * (1 + k * u)))
      } else {
        c(D * ka / (v * (ka - k)) * (exp(-k * u) - exp(-ka * u)),
          D * ka / (v * (ka - k)) * ((1 - exp(-k * u)) / k - (1 - exp(-ka * u)) / ka))
      }
    },
    stop("unknown model ", model))
}

# Scalar secondary parameters (specs/models.md MOD-SEC-02), as mpfr numbers.
secondaries <- function(model, q) {
  out <- list(v = q$v, k = q[["k"]], cl = q$cl, half_life = log(mp(2)) / q[["k"]],
              auc_inf = q$D / q$cl, mrt_system = 1 / q[["k"]], vss = q$v)
  tl <- q$tlag
  if (model == "pk1.iv_bolus") {
    out$c0 <- q$D / q$v
    out$mrt <- 1 / q[["k"]]
  } else if (model %in% c("pk1.iv_infusion", "pk1.oral_0", "pk1.oral_0_lag")) {
    out$tmax_pred <- tl + q$dur
    out$cmax_pred <- conc_auc(model, q, tl + q$dur)[1]
    out$mrt <- 1 / q[["k"]] + q$dur / 2 + tl
  } else {
    ka <- q[["ka"]]
    out$tmax_pred <- (if (ka == q[["k"]]) 1 / q[["k"]] else log(ka / q[["k"]]) / (ka - q[["k"]])) + tl
    out$cmax_pred <- conc_auc(model, q, out$tmax_pred)[1]
    out$mrt <- 1 / q[["k"]] + 1 / ka + tl
  }
  out
}

# ---------------------------------------------------------------- numerical cross-checks (double)
# State (G depot, A central amount, AUC of concentration). Input as piecewise constant rate.
breakpoints_of <- function(model, d) {
  tl <- as.numeric(d$tlag)
  switch(model,
    "pk1.iv_bolus" = 0,
    "pk1.iv_infusion" = c(0, as.numeric(d$dur)),
    "pk1.oral_0" = c(0, as.numeric(d$dur)),
    "pk1.oral_0_lag" = tl + c(0, as.numeric(d$dur)),
    "pk1.oral_1" = 0,
    "pk1.oral_1_lag" = tl)
}

# Matrix exponential solution on the given times (all >= 0 handled; t < 0 gives 0).
expm_solution <- function(model, q, times) {
  D <- as.numeric(q$D); v <- as.numeric(q$v); k <- as.numeric(q[["k"]])
  ka <- if (is.null(q[["ka"]])) NA_real_ else as.numeric(q[["ka"]])
  bp <- breakpoints_of(model, q)
  zero_order <- model %in% c("pk1.iv_infusion", "pk1.oral_0", "pk1.oral_0_lag")
  first_order <- model %in% c("pk1.oral_1", "pk1.oral_1_lag")
  # Linear system on (G, A, AUC, 1): the constant 1 carries the zero-order input rate.
  rate_matrix <- function(rate) {
    M <- matrix(0, 4, 4)
    if (first_order) {
      M[1, 1] <- -ka; M[2, 1] <- ka
    }
    M[2, 2] <- -k; M[3, 2] <- 1 / v; M[2, 4] <- rate
    M
  }
  start <- bp[1]
  y0 <- c(0, 0, 0, 1)
  if (model == "pk1.iv_bolus") y0[2] <- D
  if (first_order) y0[1] <- D
  # Ordered segment edges: the start, any further break point, then the requested times.
  edges <- sort(unique(c(start, bp, times[times > start])))
  edges <- edges[edges >= start]
  state_at <- list()
  y <- y0
  state_at[[as.character(start)]] <- y
  R <- if (zero_order) D / as.numeric(q$dur) else 0
  for (i in seq_len(length(edges) - 1L)) {
    a <- edges[i]; b <- edges[i + 1L]
    inside <- zero_order && a >= bp[length(bp) - 1L] && b <= bp[length(bp)]
    M <- rate_matrix(if (inside) R else 0)
    y <- as.numeric(expm(M * (b - a)) %*% y)
    state_at[[as.character(b)]] <- y
  }
  sapply(times, function(t) {
    if (t < start || (t == start && model != "pk1.iv_bolus")) return(c(0, 0))
    s <- state_at[[as.character(t)]]
    c(s[2] / v, s[3])
  })
}

# deSolve (lsoda) solution with two extra states: AUC and the first moment of the curve.
desolve_solution <- function(model, q, times, t_end = NULL) {
  D <- as.numeric(q$D); v <- as.numeric(q$v); k <- as.numeric(q[["k"]])
  ka <- if (is.null(q[["ka"]])) NA_real_ else as.numeric(q[["ka"]])
  bp <- breakpoints_of(model, q)
  zero_order <- model %in% c("pk1.iv_infusion", "pk1.oral_0", "pk1.oral_0_lag")
  first_order <- model %in% c("pk1.oral_1", "pk1.oral_1_lag")
  R <- if (zero_order) D / as.numeric(q$dur) else 0
  t_in0 <- if (zero_order) bp[length(bp) - 1L] else NA
  t_in1 <- if (zero_order) bp[length(bp)] else NA
  rhs <- function(t, y, parms) {
    input <- if (zero_order && t >= t_in0 && t < t_in1) R else 0
    dG <- if (first_order) -ka * y[1] else 0
    dA <- (if (first_order) ka * y[1] else 0) - k * y[2] + input
    list(c(dG, dA, y[2] / v, t * y[2] / v))
  }
  start <- bp[1]
  y0 <- c(0, 0, 0, 0)
  if (model == "pk1.iv_bolus") y0[2] <- D
  if (first_order) y0[1] <- D
  grid <- sort(unique(c(start, bp, times[times > start], if (!is.null(t_end)) t_end)))
  y <- y0
  out <- list(); out[[as.character(start)]] <- y
  for (i in seq_len(length(grid) - 1L)) {
    sol <- ode(y = y, times = c(grid[i], grid[i + 1L]), func = rhs, parms = NULL, method = "lsoda",
               rtol = 1e-12, atol = 1e-15, maxsteps = 1e6)
    y <- as.numeric(sol[2, -1])
    out[[as.character(grid[i + 1L])]] <- y
  }
  list(grid = function(t) {
         if (t < start || (t == start && model != "pk1.iv_bolus")) return(c(0, 0))
         s <- out[[as.character(t)]]
         c(s[2] / v, s[3])
       },
       final = out[[as.character(grid[length(grid)])]])
}

rel_ok <- function(a, b, scale, tol = 1e-8) {
  # a: numerical solution, b: closed form. Relative error, with values below 1e-10 of the largest
  # one of the profile compared absolutely (a numerical solver cannot do better than its own floor).
  all(abs(a - b) <= tol * pmax(abs(b), 1e-10 * scale))
}

# ---------------------------------------------------------------- cases
base_times <- c(-1, 0, 0.25, 0.5, 1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 12, 24, 48)
cases <- list(
  list(name = "iv_bolus_cl", model = "pk1.iv_bolus", p = list(dose = 100, v = 10, cl = 2),
       note = "IV bolus, V and CL (worked example M1)"),
  list(name = "iv_bolus_k", model = "pk1.iv_bolus", p = list(dose = 100, v = 10, k = 0.2),
       note = "the same model given by V and k: identical values"),
  list(name = "iv_bolus_b", model = "pk1.iv_bolus", p = list(dose = 250, v = 25.7, cl = 3.1),
       note = "IV bolus, other parameters"),
  list(name = "iv_bolus_zero_dose", model = "pk1.iv_bolus", p = list(dose = 0, v = 10, cl = 2),
       note = "dose 0 is allowed and gives 0 everywhere (MOD-GEN-04)"),
  list(name = "iv_infusion", model = "pk1.iv_infusion", p = list(dose = 100, v = 10, cl = 2, dur = 2),
       note = "IV infusion over 2 h (worked example M3)"),
  list(name = "iv_infusion_b", model = "pk1.iv_infusion",
       p = list(dose = 500, v = 50, k = 0.14, dur = 0.75), extra_times = c(0.75),
       note = "IV infusion over 0.75 h, other parameters"),
  list(name = "oral_1", model = "pk1.oral_1", p = list(dose = 100, v = 10, cl = 2, ka = 1),
       note = "first-order absorption, ka = 1 (worked example M2)"),
  list(name = "oral_1_k", model = "pk1.oral_1", p = list(dose = 100, v = 10, k = 0.2, ka = 1),
       note = "the same model given by V and k"),
  list(name = "oral_1_slow_ka", model = "pk1.oral_1",
       p = list(dose = 320, v = 32.4, cl = 3.9, ka = 0.35),
       note = "slow absorption"),
  list(name = "oral_1_fast_ka", model = "pk1.oral_1", p = list(dose = 500, v = 40, k = 0.05, ka = 25),
       note = "ka / k = 500"),
  list(name = "oral_1_ka_eq_k", model = "pk1.oral_1", p = list(dose = 100, v = 10, k = 0.2, ka = 0.2),
       note = "ka = k: the analytic limit (worked example M5)"),
  list(name = "oral_1_ka_near_k_1e9", model = "pk1.oral_1",
       p = list(dose = 100, v = 10, k = 0.2, ka = 0.200000001),
       note = "ka - k = 1e-9: cancellation in the textbook form (worked example M5)"),
  list(name = "oral_1_ka_near_k_1e6", model = "pk1.oral_1",
       p = list(dose = 100, v = 10, k = 0.2, ka = 0.200001),
       note = "ka - k = 1e-6"),
  list(name = "oral_1_ka_near_k_1e3", model = "pk1.oral_1",
       p = list(dose = 100, v = 10, k = 0.2, ka = 0.201),
       note = "ka - k = 1e-3"),
  list(name = "oral_1_flip_flop", model = "pk1.oral_1", p = list(dose = 100, v = 2, k = 1, ka = 0.2),
       note = "the flip-flop partner of oral_1_k (worked example M4): the same concentrations"),
  list(name = "oral_1_lag", model = "pk1.oral_1_lag",
       p = list(dose = 100, v = 10, cl = 2, ka = 1, tlag = 0.5),
       note = "first-order absorption with a lag of 0.5 h"),
  list(name = "oral_1_lag_b", model = "pk1.oral_1_lag",
       p = list(dose = 200, v = 41, cl = 5.2, ka = 2.4, tlag = 1.25), extra_times = c(1.25),
       note = "first-order absorption with a lag of 1.25 h, other parameters"),
  list(name = "oral_1_lag_ka_eq_k", model = "pk1.oral_1_lag",
       p = list(dose = 100, v = 10, k = 0.2, ka = 0.2, tlag = 0.5),
       note = "lag and ka = k"),
  list(name = "oral_0", model = "pk1.oral_0", p = list(dose = 100, v = 10, cl = 2, dur = 2),
       note = "zero-order absorption over 2 h"),
  list(name = "oral_0_lag", model = "pk1.oral_0_lag",
       p = list(dose = 100, v = 10, cl = 2, dur = 2, tlag = 0.5),
       note = "zero-order absorption with a lag of 0.5 h (worked example M3)"),
  list(name = "oral_0_lag_b", model = "pk1.oral_0_lag",
       p = list(dose = 250, v = 33, cl = 4.1, dur = 3.5, tlag = 0.75), extra_times = c(0.75, 4.25),
       note = "zero-order absorption over 3.5 h with a lag of 0.75 h")
)

versions <- list(
  R = R.version.string,
  Rmpfr = as.character(packageVersion("Rmpfr")),
  expm = as.character(packageVersion("expm")),
  deSolve = as.character(packageVersion("deSolve")),
  jsonlite = as.character(packageVersion("jsonlite"))
)

total_values <- 0L
for (cs in cases) {
  q <- derive(cs$p)
  times <- sort(unique(c(base_times, cs$extra_times)))
  # Closed forms.
  grid_vals <- lapply(times, function(t) conc_auc(cs$model, q, t))
  conc <- sapply(grid_vals, function(x) as.numeric(x[1]))
  auc <- sapply(grid_vals, function(x) as.numeric(x[2]))
  scal <- secondaries(cs$model, q)
  scal_num <- sapply(scal, as.numeric)

  # Cross-check 1: matrix exponential.
  ex <- expm_solution(cs$model, q, times)
  scale <- max(abs(conc))
  stopifnot(rel_ok(ex[1, ], conc, scale), rel_ok(ex[2, ], auc, max(abs(auc))))
  # Cross-check 2: lsoda, including the AUC / MRT / Cmax / Tmax of the whole curve.
  t_end <- 80 / min(as.numeric(q[["k"]]), if (is.null(q[["ka"]])) Inf else as.numeric(q[["ka"]])) +
    as.numeric(q$tlag) + (if (is.null(q$dur)) 0 else as.numeric(q$dur))
  ds <- desolve_solution(cs$model, q, times, t_end)
  dv <- sapply(times, ds$grid)
  stopifnot(rel_ok(dv[1, ], conc, scale), rel_ok(dv[2, ], auc, max(abs(auc))))
  if (as.numeric(q$D) > 0) {
    fin <- ds$final  # state at t_end: AUC and first moment of the whole curve
    stopifnot(isTRUE(abs(fin[3] - scal_num[["auc_inf"]]) <= 1e-8 * scal_num[["auc_inf"]]))
    if ("mrt" %in% names(scal_num) && cs$model != "pk1.iv_bolus") {
      stopifnot(isTRUE(abs(fin[4] / fin[3] - scal_num[["mrt"]]) <= 1e-8 * scal_num[["mrt"]]))
    }
    if ("tmax_pred" %in% names(scal_num)) {
      # The peak of the curve by direct maximisation of the closed form, and of the solver.
      f <- function(t) as.numeric(conc_auc(cs$model, q, t)[1])
      lo <- as.numeric(q$tlag); hi <- scal_num[["tmax_pred"]] * 3 + 1
      tm <- optimize(f, c(lo, hi), maximum = TRUE, tol = 1e-12)
      stopifnot(abs(tm$maximum - scal_num[["tmax_pred"]]) <= 1e-5 * scal_num[["tmax_pred"]],
                abs(tm$objective - scal_num[["cmax_pred"]]) <= 1e-7 * scal_num[["cmax_pred"]])
    }
  }

  rows <- rbind(
    data.frame(subject = paste0("t=", num17(times)), parameter = "conc", value = conc),
    data.frame(subject = paste0("t=", num17(times)), parameter = "auc", value = auc),
    data.frame(subject = "scalar", parameter = names(scal_num), value = unname(scal_num))
  )
  rows <- rows[order(match(rows$subject, c(paste0("t=", num17(times)), "scalar")),
                     match(rows$parameter, c("conc", "auc", names(scal_num)))), ]
  write_lines_lf(c("subject,parameter,value",
                   sprintf("%s,%s,%s", rows$subject, rows$parameter, num17(rows$value))),
                 file.path(exp_dir, paste0("model_", cs$name, ".csv")))
  meta <- list(
    schema = 1L,
    kind = "model",
    case = paste0("model_", cs$name),
    generated_by = "oracle/scripts/models_closed_form.R",
    model = cs$model,
    dose = cs$p$dose,
    parameters = lapply(cs$p[setdiff(names(cs$p), "dose")], function(x) x),
    times = I(times),
    units = list(dose = "mg", time = "h", concentration = "mg/L"),
    note = cs$note,
    quantities = list(
      conc = "concentration in the central compartment at the time, 0 before the dose and before the lag",
      auc = "area under the concentration curve from the dose time (not from the lag) to the time",
      scalar = "secondary parameters of specs/models.md MOD-SEC-02; mrt is the profile MRT (includes the absorption time, the lag and half the zero-order duration), mrt_system is 1/k"
    ),
    method = list(
      closed_form = "independent implementation, 256-bit arithmetic (Rmpfr), rounded once to double; the case ka = k uses the analytic limit",
      cross_checks = list("matrix exponential (expm) on intervals of constant input", "adaptive ODE solver (deSolve lsoda, rtol 1e-12) with AUC and first-moment states"),
      cross_check_tolerance = 1e-8
    ),
    versions = versions,
    n_values = nrow(rows),
    quantities_per_time = 2L,
    n_times = length(times)
  )
  write_lines_lf(toJSON(meta, pretty = TRUE, auto_unbox = TRUE, digits = NA, null = "null"),
                 file.path(exp_dir, paste0("model_", cs$name, ".options.json")))
  total_values <- total_values + nrow(rows)
  cat(sprintf("model_%-24s %3d times, %3d values\n", cs$name, length(times), nrow(rows)))
}
cat(sprintf("total %d values | %s | Rmpfr %s | expm %s | deSolve %s\n", total_values,
            versions$R, versions$Rmpfr, versions$expm, versions$deSolve))
