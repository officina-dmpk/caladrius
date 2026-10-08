#!/usr/bin/env Rscript
# Caladrius fitting oracle (task T-009, part b).
#
# Weighted least-squares reference fits of one-compartment models, with the full output set of
# specs/fit.md section 7, computed with R:
#   - estimates: stats::nls (Gauss-Newton, the primary optimizer) and minpack.lm::nlsLM
#     (Levenberg-Marquardt, the second optimizer); the script requires them to agree and writes
#     the nlsLM estimates;
#   - statistics (standard errors, intervals, covariance, correlation, eigenvalues, condition
#     numbers, residual sum of squares, AIC, SBC...): the script's own implementation of the
#     formulas of specs/fit.md FIT-OUT-01 to FIT-OUT-07 on the analytic Jacobian, cross-checked
#     against summary.nls (standard errors, residual standard error) for the weights of the fit.
#
# Data: Theoph (oral, first-order absorption, one fit per subject; the sample at time 0 is not
# fitted: the model predicts 0 there, which no weight on predicted values can use), Indometh (IV
# bolus, one fit per subject) and the five-point example of specs/fit.md section 11 (F1 to F3).
# Weighting schemes: uniform, inv_y, inv_y2, inv_yhat, inv_yhat2 (FIT-WGT-01). The predicted-value
# schemes are the iteratively reweighted fixed point of FIT-WGT-03: the weights are those of the
# solution and are constants when the derivatives are formed.
#
# Usage, from the repository root:
#   Rscript oracle/scripts/fit_wls.R
# Writes (deterministically, no timestamps):
#   oracle/expected/fit/fit_<dataset>_<weighting>.csv           long table: subject,parameter,value
#   oracle/expected/fit/fit_<dataset>_<weighting>.options.json  model, weighting, start, options, versions

suppressPackageStartupMessages({
  library(minpack.lm)
  library(jsonlite)
})

args_all <- commandArgs(trailingOnly = FALSE)
file_arg <- sub("^--file=", "", args_all[grepl("^--file=", args_all)])
root <- if (length(file_arg) == 1L) {
  normalizePath(file.path(dirname(file_arg), "..", ".."), winslash = "/", mustWork = TRUE)
} else {
  normalizePath(".", winslash = "/", mustWork = TRUE)
}
data_dir <- file.path(root, "oracle", "data")
dir.create(exp_dir <- file.path(root, "oracle", "expected", "fit"), showWarnings = FALSE, recursive = TRUE)
exp_dir <- file.path(root, "oracle", "expected", "fit")

write_lines_lf <- function(lines, path) {
  con <- file(path, open = "wb")
  on.exit(close(con))
  writeLines(lines, con, sep = "\n", useBytes = TRUE)
}
num <- function(x) ifelse(is.na(x), "", sprintf("%.15g", x))

# ---------------------------------------------------------------- models
# Closed forms (specs/models.md MOD-IVB-01, MOD-AB1-01) and their analytic derivatives, written
# out by hand and checked against central differences.
iv_fn <- function(t, dose, theta) dose / theta[["v"]] * exp(-theta[["k"]] * t)
iv_grad <- function(t, dose, theta) {
  f <- iv_fn(t, dose, theta)
  cbind(v = -f / theta[["v"]], k = -t * f)
}
oral_fn <- function(t, dose, theta) {
  v <- theta[["v"]]; k <- theta[["k"]]; ka <- theta[["ka"]]
  dose * ka / (v * (ka - k)) * (exp(-k * t) - exp(-ka * t))
}
oral_grad <- function(t, dose, theta) {
  v <- theta[["v"]]; k <- theta[["k"]]; ka <- theta[["ka"]]
  B <- dose * ka / (v * (ka - k))
  f <- oral_fn(t, dose, theta)
  cbind(v = -f / v,
        k = -B * t * exp(-k * t) + f / (ka - k),
        ka = B * t * exp(-ka * t) - f * k / (ka * (ka - k)))
}
# Model functions with a gradient attribute, for nls and nlsLM.
iv_fg <- function(t, dose, v, k) {
  th <- c(v = v, k = k)
  val <- iv_fn(t, dose, th)
  attr(val, "gradient") <- iv_grad(t, dose, th)
  val
}
oral_fg <- function(t, dose, v, k, ka) {
  th <- c(v = v, k = k, ka = ka)
  val <- oral_fn(t, dose, th)
  attr(val, "gradient") <- oral_grad(t, dose, th)
  val
}
model_defs <- list(
  "pk1.iv_bolus" = list(pars = c("v", "k"), fn = iv_fn, grad = iv_grad, fg = "iv_fg"),
  "pk1.oral_1" = list(pars = c("v", "k", "ka"), fn = oral_fn, grad = oral_grad, fg = "oral_fg")
)
# Gradient check against central differences (relative 1e-6).
for (m in names(model_defs)) {
  md <- model_defs[[m]]
  th <- if (m == "pk1.oral_1") c(v = 30, k = 0.1, ka = 1.5) else c(v = 10, k = 0.2)
  tt <- c(0.5, 1, 2, 4, 8)
  for (j in seq_along(th)) {
    h <- 1e-6 * th[[j]]
    up <- th; dn <- th; up[[j]] <- up[[j]] + h; dn[[j]] <- dn[[j]] - h
    numd <- (md$fn(tt, 100, up) - md$fn(tt, 100, dn)) / (2 * h)
    stopifnot(max(abs(numd - md$grad(tt, 100, th)[, j]) / abs(numd)) < 1e-6)
  }
}

# ---------------------------------------------------------------- statistics (FIT-OUT-*)
weights_of <- function(weighting, y, yhat) {
  switch(weighting,
    uniform = rep(1, length(y)),
    inv_y = 1 / y,
    inv_y2 = 1 / y^2,
    inv_yhat = 1 / yhat,
    inv_yhat2 = 1 / yhat^2,
    stop("unknown weighting ", weighting))
}

statistics <- function(md, t, y, dose, theta, w, alpha = 0.05) {
  P <- length(theta); N <- length(y); DF <- N - P
  yhat <- md$fn(t, dose, theta)
  r <- y - yhat
  wrss <- sum(w * r^2)
  J <- sqrt(w) * md$grad(t, dose, theta)
  JtJ <- crossprod(J)
  JtJi <- solve(JtJ)
  s2 <- wrss / DF
  cov <- s2 * JtJi
  se <- sqrt(diag(cov))
  names(se) <- names(theta)
  tq <- qt(1 - alpha / 2, DF)
  pq <- sqrt(P * qf(1 - alpha, P, DF))
  cor <- cov / outer(se, se)
  ev <- eigen(cor, symmetric = TRUE)$values
  Js <- sweep(J, 2, sqrt(colSums(J^2)), "/")
  evj <- eigen(crossprod(Js), symmetric = TRUE)$values
  out <- list()
  put <- function(name, value) out[[name]] <<- value
  put("n", N); put("p", P); put("df", DF)
  for (j in seq_len(P)) {
    nm <- names(theta)[j]
    put(paste0("estimate.", nm), theta[[j]])
    put(paste0("se.", nm), se[[j]])
    put(paste0("cv_percent.", nm), 100 * se[[j]] / abs(theta[[j]]))
    put(paste0("ci_lo.", nm), theta[[j]] - tq * se[[j]])
    put(paste0("ci_hi.", nm), theta[[j]] + tq * se[[j]])
    put(paste0("planar_lo.", nm), theta[[j]] - pq * se[[j]])
    put(paste0("planar_hi.", nm), theta[[j]] + pq * se[[j]])
  }
  for (a in seq_len(P)) for (b in a:P) {
    put(paste0("covariance.", names(theta)[a], ".", names(theta)[b]), cov[a, b])
    if (b > a) put(paste0("correlation.", names(theta)[a], ".", names(theta)[b]), cor[a, b])
  }
  for (j in seq_len(P)) put(paste0("eigenvalue.", j), ev[j])
  put("condition_number", max(ev) / min(ev))
  put("kappa_jacobian", sqrt(max(evj) / min(evj)))
  yw <- sum(w * y) / sum(w)
  fw <- sum(w * yhat) / sum(w)
  put("wrss", wrss)
  put("s", sqrt(s2))
  put("ss_weighted", sum(w * y^2))
  put("ss_corrected", sum(w * (y - yw)^2))
  put("corr_obs_pred", sum(w * (y - yw) * (yhat - fw)) /
        sqrt(sum(w * (y - yw)^2) * sum(w * (yhat - fw)^2)))
  put("aic", N * log(wrss) + 2 * P)
  put("sbc", N * log(wrss) + P * log(N))
  # Secondary parameters by the delta method (FIT-OUT-06).
  v <- theta[["v"]]; k <- theta[["k"]]
  sec <- list(
    cl = list(value = v * k, grad = c(k, v)),
    half_life = list(value = log(2) / k, grad = c(0, -log(2) / k^2)),
    auc_inf = list(value = dose / (v * k), grad = c(-dose / (v^2 * k), -dose / (v * k^2)))
  )
  for (nm in names(sec)) {
    g <- c(sec[[nm]]$grad, rep(0, P - 2L))
    sg <- sqrt(as.numeric(t(g) %*% cov %*% g))
    put(paste0("estimate.", nm), sec[[nm]]$value)
    put(paste0("se.", nm), sg)
    put(paste0("cv_percent.", nm), 100 * sg / abs(sec[[nm]]$value))
    put(paste0("ci_lo.", nm), sec[[nm]]$value - tq * sg)
    put(paste0("ci_hi.", nm), sec[[nm]]$value + tq * sg)
  }
  out
}

# ---------------------------------------------------------------- optimizers
# The data frame holds t, y, dose (constant) and the weights w for the call.
fit_nls <- function(md, d, start, w, lm) {
  fm <- as.formula(paste0("y ~ ", md$fg, "(t, dose, ", paste(md$pars, collapse = ", "), ")"))
  d$w <- w
  if (lm) {
    nlsLM(fm, data = d, start = start, weights = w,
          control = nls.lm.control(maxiter = 1000, ftol = 1e-15, ptol = 1e-15, gtol = 0))
  } else {
    nls(fm, data = d, start = start, weights = w,
        control = nls.control(maxiter = 500, tol = 1e-7, minFactor = 1e-12, scaleOffset = 0))
  }
}

# Fixed weights: one call. Predicted-value weights: iterate to the fixed point.
solve_fit <- function(md, d, weighting, start, lm) {
  run <- function(w, st) {
    f <- fit_nls(md, d, st, w, lm)
    cf <- coef(f)
    setNames(as.numeric(cf), names(cf))
  }
  if (weighting %in% c("uniform", "inv_y", "inv_y2")) {
    w <- weights_of(weighting, d$y, NULL)
    th <- run(w, start)
    return(list(theta = th, w = w, iterations = 1L, by = "direct"))
  }
  # Predicted-value weights (FIT-WGT-03). A fixed point is a theta with J(theta)' W(theta) r(theta) = 0,
  # W from the predictions at theta. Three steps, in this order:
  #   1. fixed-point iteration (refit with frozen weights, reweight) until it settles;
  #   2. if it has not settled in 100 rounds (it can cycle, or contract slowly, even when a fixed
  #      point exists), root finding on the stationarity equations from where it stopped;
  #   3. if that does not give a fixed point, root finding from a fixed grid of starting points
  #      around the initial estimates.
  # A candidate is accepted only if it passes `is_fixed_point`: the stationarity sums
  # sum_i J_ij w_i r_i are small compared with the sum of the absolute values of their terms (a
  # measure of cancellation that does not depend on the scale of the weights). Dividing by a quantity
  # that grows with the weights (sqrt(sum w y^2)) accepted degenerate "solutions" with predictions
  # of 1e-90 and weights of 1e90 (task T-018).
  stationarity_terms <- function(par) {
    par <- setNames(as.numeric(par), md$pars)
    yh <- md$fn(d$t, d$dose, par)
    w <- weights_of(weighting, d$y, yh)
    J <- md$grad(d$t, d$dose, par)
    t(J * (w * (d$y - yh)))  # row j: the terms of sum_i J_ij w_i r_i
  }
  is_fixed_point <- function(par, tol = 1e-10) {
    par <- as.numeric(par)
    if (any(!is.finite(par)) || any(par <= 0)) return(FALSE)
    terms <- stationarity_terms(par)
    if (any(!is.finite(terms))) return(FALSE)
    all(abs(rowSums(terms)) <= tol * rowSums(abs(terms)))
  }
  th <- unlist(start)
  done <- FALSE
  for (it in 1:100) {
    w <- weights_of(weighting, d$y, md$fn(d$t, d$dose, th))
    thn <- run(w, as.list(th))
    done <- max(abs(thn / th - 1)) < 1e-13
    th <- thn
    if (done) break
  }
  by <- "iteration"
  # The iteration is accepted when it settled AND the stationarity sums cancel to 1e-6 of their
  # terms (a settled iteration is accurate to about 1e-8; a degenerate point is nowhere near).
  if (!done || !is_fixed_point(th, 1e-6)) {
    by <- "root_finding"
    # Constant scale of each equation (from the start point): the roots do not depend on it.
    scale0 <- pmax(rowSums(abs(stationarity_terms(unlist(start)))), .Machine$double.xmin)
    f <- function(par) rowSums(stationarity_terms(par)) / scale0
    # The stationarity equations as the first version of this script scaled them (by a quantity
    # that depends on the weights): still the first try, so that the fixed points it found
    # genuinely keep their digits; every result is judged by `is_fixed_point` alone.
    f_old <- function(par) {
      terms <- stationarity_terms(par)
      yh <- md$fn(d$t, d$dose, setNames(as.numeric(par), md$pars))
      rowSums(terms) / sqrt(sum(weights_of(weighting, d$y, yh) * d$y^2))
    }
    solve_from <- function(par0, fn = f) {
      o <- tryCatch(nls.lm(par = par0, fn = fn,
                           control = nls.lm.control(maxiter = 1000, ftol = 1e-30, ptol = 1e-30, gtol = 1e-30)),
                    error = function(e) NULL)
      if (is.null(o) || !is_fixed_point(o$par)) NULL else o
    }
    root <- solve_from(th, f_old)
    if (is.null(root)) root <- solve_from(th)
    if (is.null(root)) {
      st0 <- unlist(start)
      grid <- expand.grid(rep(list(c(0.25, 0.5, 1, 2, 4)), length(st0)))
      roots <- list()
      for (i in seq_len(nrow(grid))) {
        r <- solve_from(st0 * as.numeric(grid[i, ]))
        if (!is.null(r)) roots[[length(roots) + 1L]] <- r
      }
      if (!length(roots)) return(NULL)  # no fixed point found from any start
      # Distinct roots, if there were several: keep the one with the smallest weighted sum of squares.
      wrss_of <- function(r) {
        yh <- md$fn(d$t, d$dose, r$par)
        sum(weights_of(weighting, d$y, yh) * (d$y - yh)^2)
      }
      root <- roots[[which.min(vapply(roots, wrss_of, numeric(1)))]]
      stopifnot(all(vapply(roots, function(r) max(abs(r$par / root$par - 1)) < 1e-6, logical(1))))
    }
    th <- setNames(root$par, md$pars)
    it <- it + root$niter
  }
  w <- weights_of(weighting, d$y, md$fn(d$t, d$dose, th))
  list(theta = th, w = w, iterations = it, by = by)
}

# ---------------------------------------------------------------- datasets and cases
read_profile <- function(path) {
  d <- read.csv(path, stringsAsFactors = FALSE)
  d[order(d$subject, d$time), ]
}
weightings <- c("uniform", "inv_y", "inv_y2", "inv_yhat", "inv_yhat2")
datasets <- list(
  list(name = "theoph", file = "theoph.csv", model = "pk1.oral_1", min_time_exclusive = 0,
       note = "Theoph, oral theophylline, first-order absorption; the sample at time 0 is not fitted"),
  list(name = "indometh", file = "indometh.csv", model = "pk1.iv_bolus", min_time_exclusive = NULL,
       note = "Indometh, IV bolus (dose 25 mg is a test constant); a one-compartment model on a biexponential profile, a poor but legitimate fit"),
  list(name = "spec", file = "fit/spec.csv", model = "pk1.iv_bolus", min_time_exclusive = NULL,
       note = "the five-point example of specs/fit.md section 11 (F1 to F3)")
)

# Initial estimates: the uniform fit moved by fixed factors, rounded; the same for every weighting
# of a subject, and far enough from the minimum to need several iterations.
start_of <- function(md, theta) {
  s <- as.list(theta)
  s$v <- round(1.25 * theta[["v"]], 1)
  s$k <- round(0.8 * theta[["k"]], 3)
  if (!is.null(s$ka)) s$ka <- round(0.7 * theta[["ka"]], 2)
  s
}

versions <- list(
  R = R.version.string,
  minpack.lm = as.character(packageVersion("minpack.lm")),
  jsonlite = as.character(packageVersion("jsonlite"))
)

total_values <- 0L
for (ds in datasets) {
  md <- model_defs[[ds$model]]
  prof <- read_profile(file.path(data_dir, ds$file))
  subjects <- sort(unique(prof$subject))
  starts <- list()
  # Starts come from the uniform fit by nlsLM from a generic point.
  for (s in subjects) {
    d <- prof[prof$subject == s, ]
    if (!is.null(ds$min_time_exclusive)) d <- d[d$time > ds$min_time_exclusive, ]
    d <- data.frame(t = d$time, y = d$conc, dose = d$dose)
    generic <- if (ds$model == "pk1.oral_1") list(v = 30, k = 0.1, ka = 1.5) else list(v = 10, k = 0.2)
    base <- solve_fit(md, d, "uniform", generic, TRUE)
    starts[[as.character(s)]] <- start_of(md, base$theta)
  }
  for (wg in weightings) {
    rows <- list()
    agree <- list()
    by <- list()
    no_fixed_point <- list()
    for (s in subjects) {
      d <- prof[prof$subject == s, ]
      if (!is.null(ds$min_time_exclusive)) d <- d[d$time > ds$min_time_exclusive, ]
      d <- data.frame(t = d$time, y = d$conc, dose = d$dose)
      st <- starts[[as.character(s)]]
      lmfit <- solve_fit(md, d, wg, st, TRUE)
      if (is.null(lmfit)) {
        no_fixed_point[[length(no_fixed_point) + 1L]] <- as.character(s)
        next
      }
      by[[as.character(s)]] <- lmfit$by
      gn <- tryCatch(solve_fit(md, d, wg, st, FALSE), error = function(e) NULL)
      if (!is.null(gn) && !is.list(gn)) gn <- NULL
      diff <- if (is.null(gn)) NA_real_ else max(abs(gn$theta / lmfit$theta - 1))
      agree[[as.character(s)]] <- list(nls_converged = !is.null(gn), max_rel_difference = if (is.na(diff)) NULL else signif(diff, 1))
      if (!is.null(gn)) stopifnot(diff < 1e-5)
      th <- lmfit$theta
      w <- lmfit$w
      # Cross-check with summary.nls for the weights of the solution (frozen).
      chk <- fit_nls(md, d, as.list(th), w, TRUE)
      sm <- summary(chk)
      st_out <- statistics(md, d$t, d$y, d$dose[1], th, w)
      for (j in seq_along(th)) {
        stopifnot(abs(sm$coefficients[j, 2] / st_out[[paste0("se.", names(th)[j])]] - 1) < 1e-6)
      }
      stopifnot(abs(sm$sigma / st_out$s - 1) < 1e-6)
      rows[[as.character(s)]] <- data.frame(subject = s, parameter = names(st_out),
                                            value = unlist(st_out), stringsAsFactors = FALSE)
    }
    long <- do.call(rbind, rows)
    case <- paste0("fit_", ds$name, "_", wg)
    write_lines_lf(c("subject,parameter,value", sprintf("%d,%s,%s", long$subject, long$parameter, num(long$value))),
                   file.path(exp_dir, paste0(case, ".csv")))
    opts <- list(
      schema = 1L,
      kind = "fit",
      case = case,
      generated_by = "oracle/scripts/fit_wls.R",
      dataset = ds$name,
      data_file = paste0("data/", ds$file),
      model = ds$model,
      weighting = wg,
      note = ds$note,
      units = list(dose = "mg", time = "h", concentration = "mg/L"),
      observations = if (is.null(ds$min_time_exclusive)) list(use = "all") else list(use = "time_greater_than", time = ds$min_time_exclusive),
      initial_estimates = lapply(starts, function(x) lapply(x, function(v) v)),
      fit_options = list(
        derivatives = "analytic",
        criterion = "relative_decrease",
        convergence = 1e-10,
        max_iterations = 200,
        confidence_level = 0.95
      ),
      weights_note = if (wg %in% c("inv_yhat", "inv_yhat2")) "iteratively reweighted fixed point (specs/fit.md FIT-WGT-03): the weights of the solution, constants in the derivatives" else "weights from the observed values (or 1)",
      method = list(
        estimates = "minpack.lm::nlsLM (Levenberg-Marquardt, ftol = ptol = 1e-15); stats::nls (Gauss-Newton, relative-offset tol 1e-7) is required to agree within 1e-5 relative where it converges from the same start",
        statistics = "oracle/scripts/fit_wls.R, formulas of specs/fit.md FIT-OUT-01 to FIT-OUT-07 on the analytic Jacobian; standard errors and S checked against summary.nls",
        optimizer_agreement = agree
      ),
      definitions = list(
        aic = "N * ln(WRSS) + 2 * P (FIT-OUT-07, an assumed form)",
        sbc = "N * ln(WRSS) + P * ln(N)",
        ss_corrected = "sum of w * (y - weighted mean of y)^2",
        corr_obs_pred = "weighted Pearson correlation of observed and predicted values, weights w",
        ci = "estimate +/- t(0.975; DF) * SE; planar: estimate +/- sqrt(P * F(0.95; P, DF)) * SE",
        eigenvalue = "eigenvalues of the correlation matrix of the estimates, decreasing; condition_number = largest / smallest; kappa_jacobian = sqrt of the same ratio for the Jacobian with unit-length columns",
        secondary = "cl = v * k, half_life = ln 2 / k, auc_inf = dose / (v * k); standard errors by the delta method"
      ),
      versions = versions,
      subjects = as.list(setdiff(as.character(subjects), unlist(no_fixed_point))),
      no_fixed_point = if (length(no_fixed_point)) I(unlist(no_fixed_point)) else list(),
      solved_by = by,
      n_values = nrow(long)
    )
    write_lines_lf(toJSON(opts, pretty = TRUE, auto_unbox = TRUE, digits = NA, null = "null"),
                   file.path(exp_dir, paste0(case, ".options.json")))
    total_values <- total_values + nrow(long)
    cat(sprintf("%-26s %2d subjects, %4d values, nls fails for %d\n", case, length(subjects), nrow(long),
                sum(!vapply(agree, function(a) a$nls_converged, logical(1)))))
  }
}
# ---------------------------------------------------------------- Gauss-Newton trace of F1
# Worked example F1 of specs/fit.md (uniform weights, analytic derivatives, full steps) from
# (V, k) = (12, 0.15): the iterates to full double precision, for the trace test of caladrius-fit.
# The specification prints them to 7 decimals; rounding alone can move a printed value by 5e-8
# relative, so the test compares with these.
{
  f1 <- read_profile(file.path(data_dir, "fit/spec.csv"))
  tt <- f1$time; yy <- f1$conc; dose <- f1$dose[1]
  th <- c(v = 12, k = 0.15)
  trace_rows <- character(0)
  for (i in 0:5) {
    r <- yy - iv_fn(tt, dose, th)
    trace_rows <- c(trace_rows, sprintf("%d,%s,%s,%s", i, num(sum(r^2)), num(th[["v"]]), num(th[["k"]])))
    J <- iv_grad(tt, dose, th)
    th <- th + as.numeric(solve(crossprod(J), crossprod(J, r)))
  }
  write_lines_lf(c("iteration,wrss,v,k", trace_rows), file.path(exp_dir, "gauss_newton_f1.csv"))
}
cat(sprintf("total %d values | %s | minpack.lm %s\n", total_values, versions$R, versions$minpack.lm))
