#!/usr/bin/env Rscript
# Caladrius two-compartment model oracle (task T-032).
#
# Ground truth for specs/models.md section 11 (MOD-2C-01..24) before any engine code exists:
#   - model_pk2_<id>_<case>        values of the six ids on time grids (conc, auc, aumc) and the
#                                  scalars (the three parameter sets, half-lives, volumes, AUC and
#                                  AUMC to infinity, MRT, Cmax and Tmax);
#   - model_pk2_deriv_<id>_<case>  partial derivatives of the concentration, in 256-bit central
#                                  differences, for the clearance, micro and macro parameter sets
#                                  and for ka, tlag and dur;
#   - model_pk2_errors             what the engine must refuse (MOD-2C-05, MOD-2C-22, MOD-GEN-04),
#                                  as expected "not available" rows with the reason.
#
# Independence from the engine (specs/models.md OM-06): the concentrations are the explicit sums of
# exponentials of MOD-2C-07, 08, 09 and the confluent limits of MOD-2C-10 evaluated in 256-bit
# arithmetic (Rmpfr) and rounded once to double. The script does NOT use the stable forms of
# MOD-2C-03/04/10 (they are not needed with 256 bits): the exponents come from the textbook
# quadratic, the weights from (alpha - k21)/(alpha - beta), the reverse conversion from the sums
# and products of the textbook. AUC(0, t) and AUMC(0, t) are the exact integrals of those sums
# (terms c * u^p * exp(-lambda * u) on each segment of constant input), not the combination of
# one-compartment areas of MOD-2C-14.
#
# Every case is cross-checked, within 1e-8 (relative to the value, values below 1e-3 of the
# largest one of the profile compared absolutely at 1e-11 of it), against two numerical solutions of the three
# differential equations of MOD-2C-01:
#   - the matrix exponential (package expm) on each interval of constant input, with extra states
#     for AUC and for the integral of the AUC (AUMC = t * AUC - integral of AUC);
#   - an adaptive solver (package deSolve, lsoda, rtol 1e-12) with states for the AUC and the first
#     moment, which also checks AUC(0, inf), AUMC(0, inf), the MRT, Tmax and Cmax.
# The exponents are also compared with the eigenvalues of the 2 x 2 system where the two are not
# nearly equal. No random numbers are used.
#
# Usage, from the repository root:
#   Rscript oracle/scripts/models_2c_closed_form.R
# Writes (deterministically, no timestamps) into oracle/expected/models/pk2/:
#   model_pk2_<...>.csv           long table: subject,parameter,value (subject: t=<time> or scalar)
#   model_pk2_<...>.options.json  model id, dose, parameters, times, quantities, method, versions

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
exp_dir <- file.path(root, "oracle", "expected", "models", "pk2")
dir.create(exp_dir, showWarnings = FALSE, recursive = TRUE)

write_lines_lf <- function(lines, path) {
  con <- file(path, open = "wb")
  on.exit(close(con))
  writeLines(lines, con, sep = "\n", useBytes = TRUE)
}
num17 <- function(x) sprintf("%.17g", x)
# Shortest decimal text that reads back as the same double.
rt <- function(x) {
  s <- ""
  for (d in 15:17) {
    s <- sprintf(paste0("%.", d, "g"), x)
    if (as.numeric(s) == x) return(s)
  }
  s
}

# ---------------------------------------------------------------- minimal JSON writer
# jsonlite would print numbers with at most 15 significant digits; the parameters must read back as
# the very doubles the expected values were computed from.
jstr <- function(s) {
  s <- gsub("\\\\", "\\\\\\\\", s)
  s <- gsub("\"", "\\\\\"", s)
  paste0("\"", s, "\"")
}
jenc <- function(x, ind = 0L) {
  pad <- strrep("  ", ind)
  pad1 <- strrep("  ", ind + 1L)
  if (is.null(x)) return("null")
  if (is.list(x)) {
    if (length(x) == 0L) return(if (is.null(names(x))) "[]" else "{}")
    if (!is.null(names(x))) {
      items <- mapply(function(k, v) paste0(pad1, jstr(k), ": ", jenc(v, ind + 1L)),
                      names(x), x, USE.NAMES = FALSE)
      return(paste0("{\n", paste(items, collapse = ",\n"), "\n", pad, "}"))
    }
    return(paste0("[", paste(vapply(x, jenc, "", ind = ind + 1L), collapse = ", "), "]"))
  }
  if (is.character(x)) {
    if (length(x) == 1L) return(jstr(x))
    return(paste0("[", paste(jstr(x), collapse = ", "), "]"))
  }
  if (is.logical(x)) {
    s <- ifelse(x, "true", "false")
    return(if (length(x) == 1L) s else paste0("[", paste(s, collapse = ", "), "]"))
  }
  if (is.numeric(x)) {
    s <- if (is.integer(x)) as.character(x) else vapply(x, rt, "")
    return(if (length(x) == 1L) s else paste0("[", paste(s, collapse = ", "), "]"))
  }
  stop("jenc: unsupported value")
}
# A one-element vector that must be written as an array (not a scalar).
arr <- function(x) structure(as.list(x), class = NULL)

PREC <- 256L
mp <- function(x) mpfr(x, PREC)
zero <- function() mp(0)
TINY <- mp(2)^-150  # |ka - lambda| below this: the confluent limit of MOD-2C-10

check <- function(cond, ...) {
  if (!isTRUE(cond)) stop(paste0(..., collapse = ""), call. = FALSE)
}

# ---------------------------------------------------------------- parameter sets (256 bits)
# A parameter set is a named list; every value is read exactly as the double it was given.
#   clearance: cl, vc, q, vp          micro: k10, k12, k21, vc
#   macro: a, b, alpha, beta (the dose gives vc = D / (a + b))
#   psi: V, alpha, beta, w (the internal parameters of MOD-2C-17; weights w and 1 - w)
derive_q <- function(set, p, D, extra = list()) {
  P <- lapply(p, mp)
  D <- mp(D)
  if (set == "psi") {
    vc <- P[["V"]]
    alpha <- P[["alpha"]]; beta <- P[["beta"]]
    wa <- P[["w"]]; wb <- 1 - wa
    k21 <- alpha * wb + beta * wa
    k10 <- alpha * beta / k21
    micro <- list(vc = vc, k10 = k10, k12 = alpha + beta - k10 - k21, k21 = k21)
  } else {
    micro <- switch(set,
      clearance = list(vc = P[["vc"]], k10 = P[["cl"]] / P[["vc"]], k12 = P[["q"]] / P[["vc"]],
                       k21 = P[["q"]] / P[["vp"]]),
      micro = list(vc = P[["vc"]], k10 = P[["k10"]], k12 = P[["k12"]], k21 = P[["k21"]]),
      macro = {
        s <- P[["a"]] + P[["b"]]
        wa0 <- P[["a"]] / s
        wb0 <- P[["b"]] / s
        k21 <- P[["alpha"]] * wb0 + P[["beta"]] * wa0
        k10 <- P[["alpha"]] * P[["beta"]] / k21
        list(vc = D / s, k10 = k10, k12 = P[["alpha"]] + P[["beta"]] - k10 - k21, k21 = k21)
      },
      stop("unknown set ", set))
    vc <- micro$vc
    S <- micro$k10 + micro$k12 + micro$k21
    Pr <- micro$k10 * micro$k21
    r <- sqrt(S * S - 4 * Pr)
    alpha <- (S + r) / 2
    beta <- Pr / alpha
    wa <- (alpha - micro$k21) / (alpha - beta)
    wb <- (micro$k21 - beta) / (alpha - beta)
  }
  q <- list(D = D, vc = vc, alpha = alpha, beta = beta, wa = wa, wb = wb,
            A = D * wa / vc, B = D * wb / vc,
            ka = if (!is.null(extra[["ka"]])) mp(extra[["ka"]]) else NULL,
            tau = if (!is.null(extra[["tlag"]])) mp(extra[["tlag"]]) else zero(),
            dur = if (!is.null(extra[["dur"]])) mp(extra[["dur"]]) else NULL)
  if (!is.null(micro)) {
    q$k10 <- micro$k10; q$k12 <- micro$k12; q$k21 <- micro$k21
    q$cl <- micro$k10 * vc
    q$qq <- micro$k12 * vc
    q$vp <- q$qq / micro$k21
  }
  q
}

# ---------------------------------------------------------------- explicit forms
# A segment is a stretch of time [s0, s0 + len] (len NULL: to infinity) on which the concentration is
# a sum of terms c * u^p * exp(-lambda * u), u = t - s0.
trm <- function(c, l, p) list(c = c, l = l, p = as.integer(p))
sgm <- function(s0, len, terms) list(s0 = s0, len = len, terms = terms)

segments <- function(model, q) {
  al <- q$alpha; be <- q$beta; A <- q$A; B <- q$B; tau <- q$tau
  switch(model,
    "pk2.iv_bolus" = list(sgm(zero(), NULL, list(trm(A, al, 0L), trm(B, be, 0L)))),
    "pk2.iv_infusion" = , "pk2.oral_0" = , "pk2.oral_0_lag" = {
      # MOD-2C-08 / MOD-2C-12.
      Tt <- q$dur
      cA <- A / (al * Tt)
      cB <- B / (be * Tt)
      list(sgm(tau, Tt, list(trm(cA, zero(), 0L), trm(-cA, al, 0L),
                             trm(cB, zero(), 0L), trm(-cB, be, 0L))),
           sgm(tau + Tt, NULL, list(trm(cA * (1 - exp(-al * Tt)), al, 0L),
                                    trm(cB * (1 - exp(-be * Tt)), be, 0L))))
    },
    "pk2.oral_1" = , "pk2.oral_1_lag" = {
      ka <- q$ka
      D <- q$D; V <- q$vc
      if (abs(ka - al) < TINY) {
        f <- D * al / V   # MOD-2C-10, ka = alpha
        terms <- list(trm(f * q$wa, al, 1L), trm(f * q$wb / (al - be), be, 0L),
                      trm(-f * q$wb / (al - be), al, 0L))
      } else if (abs(ka - be) < TINY) {
        f <- D * be / V   # MOD-2C-10, ka = beta
        terms <- list(trm(f * q$wb, be, 1L), trm(f * q$wa / (al - be), be, 0L),
                      trm(-f * q$wa / (al - be), al, 0L))
      } else {
        Ao <- ka * A / (ka - al)   # MOD-2C-09
        Bo <- ka * B / (ka - be)
        terms <- list(trm(Ao, al, 0L), trm(Bo, be, 0L), trm(-(Ao + Bo), ka, 0L))
      }
      list(sgm(tau, NULL, terms))
    },
    stop("unknown model ", model))
}

eval_terms <- function(terms, u) {
  total <- zero()
  for (tm in terms) total <- total + tm$c * u^tm$p * exp(-tm$l * u)
  total
}
eval_dterms <- function(terms, u) {   # derivative in u
  total <- zero()
  for (tm in terms) {
    d <- -tm$l * u^tm$p
    if (tm$p > 0L) d <- d + tm$p * u^(tm$p - 1L)
    total <- total + tm$c * d * exp(-tm$l * u)
  }
  total
}

# Concentration at t. Before the dose and before the lag it is 0; at the start of the input of a
# non-bolus model it is 0 exactly (the terms cancel to the last bit only).
conc_at <- function(segs, t, bolus) {
  t <- mp(t)
  if (bolus && t < 0) return(zero())
  for (sg in segs) {
    if (t >= sg$s0 && (is.null(sg$len) || t <= sg$s0 + sg$len)) {
      u <- t - sg$s0
      if (!bolus && u == 0) return(zero())
      return(eval_terms(sg$terms, u))
    }
  }
  zero()
}

# integral over [0, x] (x NULL: infinity) of u^p * exp(-lam * u)
Ipow <- function(p, lam, x) {
  if (lam == 0) return(x^(p + 1L) / (p + 1L))
  f <- mp(factorial(p))
  if (is.null(x)) return(f / lam^(p + 1L))
  y <- lam * x
  s <- zero()
  for (j in 0:p) s <- s + y^j / factorial(j)
  f / lam^(p + 1L) * (1 - exp(-y) * s)
}
# AUC (m = 0) or AUMC (m = 1) from the dose time to t (t NULL: infinity).
area_at <- function(segs, t, m) {
  if (!is.null(t)) t <- mp(t)
  total <- zero()
  for (sg in segs) {
    if (!is.null(t) && t <= sg$s0) next
    upper <- if (is.null(t)) NULL else t - sg$s0
    if (!is.null(sg$len)) upper <- if (is.null(upper) || upper > sg$len) sg$len else upper
    for (tm in sg$terms) {
      v <- if (m == 0) Ipow(tm$p, tm$l, upper) else sg$s0 * Ipow(tm$p, tm$l, upper) + Ipow(tm$p + 1L, tm$l, upper)
      total <- total + tm$c * v
    }
  }
  total
}

input_mean_time <- function(model, q) {
  switch(model,
    "pk2.iv_bolus" = zero(),
    "pk2.iv_infusion" = q$dur / 2,
    "pk2.oral_0" = , "pk2.oral_0_lag" = q$dur / 2 + q$tau,
    "pk2.oral_1" = , "pk2.oral_1_lag" = 1 / q$ka + q$tau)
}

# Tmax and Cmax of the first-order models by bisection on C'(u) = 0 (MOD-2C-15 bracket).
peak_first_order <- function(q, segs) {
  ka <- q$ka; al <- q$alpha; be <- q$beta
  xof <- function(x) if (abs(ka - x) < TINY) 1 / x else log(ka / x) / (ka - x)
  lo <- xof(al); hi <- xof(be)
  if (lo > hi) { tmp <- lo; lo <- hi; hi <- tmp }
  terms <- segs[[1]]$terms
  sc <- abs(eval_dterms(terms, zero()))
  check(eval_dterms(terms, lo) >= -mp(2)^-100 * sc, "C' negative at the lower bracket")
  check(eval_dterms(terms, hi) <= mp(2)^-100 * sc, "C' positive at the upper bracket")
  for (i in 1:300) {
    mid <- (lo + hi) / 2
    if (eval_dterms(terms, mid) > 0) lo <- mid else hi <- mid
  }
  u <- (lo + hi) / 2
  list(tmax = u + q$tau, cmax = eval_terms(terms, u))
}

# Scalars of MOD-2C-13, 14, 15, 20 (256-bit), named as the vocabulary of MOD-2C-20.
scalars_of <- function(model, q, segs) {
  D <- q$D; cl <- q$cl; vss <- q$vc + q$vp
  m <- input_mean_time(model, q)
  out <- list(
    alpha = q$alpha, beta = q$beta, k10 = q$k10, k12 = q$k12, k21 = q$k21,
    cl = cl, vc = q$vc, q = q$qq, vp = q$vp,
    a = q$A, b = q$B, w_alpha = q$wa, w_beta = q$wb,
    half_life = log(mp(2)) / q$beta, half_life_alpha = log(mp(2)) / q$alpha,
    vss = vss, vz = cl / q$beta, v_extrap = q$vc / q$wb,
    auc_inf = D / cl,
    aumc_inf = D * vss / cl^2 + D / cl * m,
    mrt_system = vss / cl, mrt = vss / cl + m)
  # The closed forms of MOD-2C-13/14 must be the integrals of the explicit sums.
  if (D > 0) {
    ai <- area_at(segs, NULL, 0)
    am <- area_at(segs, NULL, 1)
    check(abs(ai - out$auc_inf) <= mp(2)^-180 * out$auc_inf, "AUC(0, inf) differs from D / CL")
    check(abs(am - out$aumc_inf) <= mp(2)^-180 * out$aumc_inf, "AUMC(0, inf) differs from MOD-2C-14")
  }
  if (model == "pk2.iv_bolus") {
    out$c0 <- D / q$vc
  } else if (model %in% c("pk2.iv_infusion", "pk2.oral_0", "pk2.oral_0_lag")) {
    out$tmax_pred <- q$tau + q$dur
    out$cmax_pred <- conc_at(segs, q$tau + q$dur, FALSE)
  } else if (D > 0) {
    pk <- peak_first_order(q, segs)
    out$tmax_pred <- pk$tmax
    out$cmax_pred <- pk$cmax
  }
  # Oral-profile coefficients (MOD-2C-09): defined and well conditioned when ka is not close to
  # alpha or beta (relative gap of at least 1e-2); the coefficient of exp(-ka t) is minus their sum.
  if (model %in% c("pk2.oral_1", "pk2.oral_1_lag") && D > 0) {
    ka <- q$ka
    if (abs(ka - q$alpha) >= 1e-2 * ka && abs(ka - q$beta) >= 1e-2 * ka) {
      out$a_oral <- ka * q$A / (ka - q$alpha)
      out$b_oral <- ka * q$B / (ka - q$beta)
    }
  }
  out
}

# ---------------------------------------------------------------- numerical cross-checks (double)
num <- function(x) if (is.null(x)) NA_real_ else as.numeric(x)
model_kind <- function(model) {
  list(zero_order = model %in% c("pk2.iv_infusion", "pk2.oral_0", "pk2.oral_0_lag"),
       first_order = model %in% c("pk2.oral_1", "pk2.oral_1_lag"),
       bolus = model == "pk2.iv_bolus")
}
breakpoints <- function(model, nq) {
  kd <- model_kind(model)
  if (kd$bolus) return(0)
  if (kd$zero_order) return(c(nq$tau, nq$tau + nq$dur))
  nq$tau
}
double_params <- function(q) {
  list(D = num(q$D), V = num(q$vc), k10 = num(q$k10), k12 = num(q$k12), k21 = num(q$k21),
       ka = num(q$ka), tau = num(q$tau), dur = num(q$dur))
}

# Matrix exponential on each interval of constant input. State (G, A1, A2, X, Z, 1): X = AUC,
# Z = integral of X, 1 carries the constant rate. AUMC = t X - Z.
expm_solution <- function(model, nq, times) {
  kd <- model_kind(model)
  bp <- breakpoints(model, nq)
  start <- bp[1]
  rate_in <- if (kd$zero_order) nq$D / nq$dur else 0
  mat <- function(rate) {
    M <- matrix(0, 6, 6)
    if (kd$first_order) { M[1, 1] <- -nq$ka; M[2, 1] <- nq$ka }
    M[2, 2] <- -(nq$k10 + nq$k12); M[2, 3] <- nq$k21; M[3, 2] <- nq$k12; M[3, 3] <- -nq$k21
    M[4, 2] <- 1 / nq$V; M[5, 4] <- 1; M[2, 6] <- rate
    M
  }
  y <- c(if (kd$first_order) nq$D else 0, if (kd$bolus) nq$D else 0, 0, 0, 0, 1)
  edges <- sort(unique(c(start, bp, times[times > start])))
  states <- list()
  states[[as.character(start)]] <- y
  for (i in seq_len(length(edges) - 1L)) {
    a <- edges[i]; b <- edges[i + 1L]
    inside <- kd$zero_order && a >= bp[1] && b <= bp[2]
    y <- as.numeric(expm(mat(if (inside) rate_in else 0) * (b - a)) %*% y)
    states[[as.character(b)]] <- y
  }
  sapply(times, function(t) {
    if (t < start || (t == start && !kd$bolus)) return(c(0, 0, 0))
    s <- states[[as.character(t)]]
    c(s[2] / nq$V, s[4], t * s[4] - s[5])
  })
}

# deSolve (lsoda) with states (G, A1, A2, AUC, first moment).
desolve_solution <- function(model, nq, times, t_end, auc_inf, aumc_inf) {
  kd <- model_kind(model)
  bp <- breakpoints(model, nq)
  start <- bp[1]
  rate_in <- if (kd$zero_order) nq$D / nq$dur else 0
  rhs <- function(t, y, parms) {
    dG <- if (kd$first_order) -nq$ka * y[1] else 0
    dA1 <- (if (kd$first_order) nq$ka * y[1] else 0) - (nq$k10 + nq$k12) * y[2] + nq$k21 * y[3] + parms
    dA2 <- nq$k12 * y[2] - nq$k21 * y[3]
    list(c(dG, dA1, dA2, y[2] / nq$V, t * y[2] / nq$V))
  }
  y <- c(if (kd$first_order) nq$D else 0, if (kd$bolus) nq$D else 0, 0, 0, 0)
  atol <- 1e-14 * c(nq$D, nq$D, nq$D, auc_inf, aumc_inf)
  grid <- sort(unique(c(start, bp, times[times > start], t_end)))
  out <- list()
  out[[as.character(start)]] <- y
  for (i in seq_len(length(grid) - 1L)) {
    a <- grid[i]; b <- grid[i + 1L]
    inside <- kd$zero_order && a >= bp[1] && b <= bp[2]
    sol <- ode(y = y, times = c(a, b), func = rhs, parms = if (inside) rate_in else 0,
               method = "lsoda", rtol = 1e-12, atol = atol, maxsteps = 1e6)
    y <- as.numeric(sol[2, -1])
    out[[as.character(b)]] <- y
  }
  list(at = function(t) {
         if (t < start || (t == start && !kd$bolus)) return(c(0, 0, 0))
         s <- out[[as.character(t)]]
         c(s[2] / nq$V, s[4], s[5])
       },
       final = out[[as.character(grid[length(grid)])]])
}

rel_ok <- function(a, b, scale, tol = 1e-8) {
  # values below 1e-3 of the largest value of the profile are compared absolutely, at 1e-11 of it:
  # the solvers keep an absolute accuracy of that order, not a relative one, on a decayed tail
  all(abs(a - b) <= tol * pmax(abs(b), 1e-3 * scale))
}

# ---------------------------------------------------------------- time grids
make_times <- function(model, q, extra) {
  al <- num(q$alpha); be <- num(q$beta)
  ta <- 1 / al; tb <- 1 / be
  # very small times (1e-9 and 1e-6 of the mean life of the fast phase) test the small-argument forms
  ts <- c(ta * c(1e-9, 1e-6, 0.01, 0.1, 0.5, 1, 2, 4), tb * c(0.1, 0.25, 0.5, 1, 2, 4, 8, 16, 40))
  if (!is.null(extra$ka)) ts <- c(ts, c(0.1, 1, 5) / extra$ka)
  exact <- c(-1, 0)
  if (!is.null(extra$tlag)) {
    tl <- extra$tlag
    ts <- c(ts, tl + ta * c(1e-6, 0.5, 2), tl + tb * c(1, 4), tl / 2)
    exact <- c(exact, tl, tl + 1e-3 * ta)
  }
  if (!is.null(extra$dur)) {
    T <- extra$dur; tl <- if (is.null(extra$tlag)) 0 else extra$tlag
    ts <- c(ts, tl + T / 2, tl + 2 * T)
    exact <- c(exact, tl + T, tl + T * 1.001)
  }
  # 1, 3, 4, 10, 12 and 24 are the times of the worked examples of specs/models.md section 11.6
  times <- sort(unique(c(signif(ts, 6), exact, c(1, 3, 4, 10, 12, 24), signif(tb * 800, 6))))
  times
}

# True when the textbook forms in double precision keep their digits: the exponents differ by at
# least 1e-2 relative, and ka (first-order input) differs from both by at least 5e-2 relative to ka.
# The test layer compares the naive implementation of the testkit with the expected values only for
# those cases.
textbook_flag <- function(q, extra) {
  ok <- (num(q$alpha) - num(q$beta)) / num(q$alpha) > 1e-2
  # the zero-order input: 1 - exp(-alpha T) cancels for a very short input
  if (!is.null(extra$dur)) ok <- ok && extra$dur * num(q$alpha) > 0.05
  if (!is.null(extra$ka)) {
    ka <- extra$ka
    ok <- ok && abs(ka - num(q$alpha)) > 5e-2 * ka && abs(ka - num(q$beta)) > 5e-2 * ka
  }
  ok
}

# ---------------------------------------------------------------- one value case
write_case <- function(name, model, set, p, dose, extra, note, textbook_ok = NULL) {
  q <- derive_q(set, p, dose, extra)
  segs <- segments(model, q)
  bolus <- model == "pk2.iv_bolus"
  times <- make_times(model, q, extra)
  conc <- sapply(times, function(t) num(conc_at(segs, t, bolus)))
  auc <- sapply(times, function(t) num(area_at(segs, t, 0)))
  aumc <- sapply(times, function(t) num(area_at(segs, t, 1)))
  sc <- scalars_of(model, q, segs)
  scn <- sapply(sc, num)

  if (dose > 0) {
    nq <- double_params(q)
    scale <- max(abs(conc)); sa <- max(abs(auc)); sm <- max(abs(aumc))
    # Cross-check 1: matrix exponential.
    ex <- expm_solution(model, nq, times)
    check(rel_ok(ex[1, ], conc, scale), name, ": expm concentration")
    check(rel_ok(ex[2, ], auc, sa), name, ": expm AUC")
    check(rel_ok(ex[3, ], aumc, sm), name, ": expm AUMC")
    # Cross-check 2: lsoda, up to a time at which everything has decayed (80 / beta).
    t_end <- (if (is.null(extra$tlag)) 0 else extra$tlag) + (if (is.null(extra$dur)) 0 else extra$dur) +
      80 / min(nq$k10, nq$k21, num(q$beta), if (is.null(extra$ka)) Inf else extra$ka)
    ds <- desolve_solution(model, nq, times, t_end, scn[["auc_inf"]], scn[["aumc_inf"]])
    dv <- sapply(times, ds$at)
    check(rel_ok(dv[1, ], conc, scale), name, ": lsoda concentration")
    check(rel_ok(dv[2, ], auc, sa), name, ": lsoda AUC")
    check(rel_ok(dv[3, ], aumc, sm), name, ": lsoda AUMC")
    fin <- ds$final
    check(abs(fin[4] - scn[["auc_inf"]]) <= 1e-8 * scn[["auc_inf"]], name, ": lsoda AUC to infinity")
    check(abs(fin[5] - scn[["aumc_inf"]]) <= 1e-8 * scn[["aumc_inf"]], name, ": lsoda AUMC to infinity")
    check(abs(fin[5] / fin[4] - scn[["mrt"]]) <= 1e-8 * scn[["mrt"]], name, ": lsoda MRT")
    # Peak: by direct maximisation of the closed form (first order), at the end of the input otherwise.
    if (!is.null(extra$ka)) {
      f <- function(t) num(conc_at(segs, t, FALSE))
      lo <- extra$tlag %||% 0
      hi <- lo + 3 * (scn[["tmax_pred"]] - lo)
      tm <- optimize(f, c(lo, hi), maximum = TRUE, tol = 1e-12)
      check(abs(tm$maximum - scn[["tmax_pred"]]) <= 1e-5 * scn[["tmax_pred"]], name, ": Tmax by optimize")
      check(abs(tm$objective - scn[["cmax_pred"]]) <= 1e-9 * scn[["cmax_pred"]], name, ": Cmax by optimize")
    }
    if (!bolus) check(all(conc <= scn[["cmax_pred"]] * (1 + 1e-12)), name, ": a grid value above Cmax")
    # Exponents against the eigenvalues of the 2 x 2 system (not for nearly equal exponents).
    if ((num(q$alpha) - num(q$beta)) / num(q$alpha) > 1e-3) {
      ev <- eigen(matrix(c(-(nq$k10 + nq$k12), nq$k12, nq$k21, -nq$k21), 2, 2), only.values = TRUE)$values
      ev <- sort(-Re(ev))
      check(abs(ev[2] - num(q$alpha)) <= 1e-9 * num(q$alpha) && abs(ev[1] - num(q$beta)) <= 1e-9 * num(q$beta),
            name, ": eigenvalues")
    }
  }

  tkeys <- paste0("t=", num17(times))
  rows <- rbind(
    data.frame(subject = tkeys, parameter = "conc", value = conc),
    data.frame(subject = tkeys, parameter = "auc", value = auc),
    data.frame(subject = tkeys, parameter = "aumc", value = aumc),
    data.frame(subject = "scalar", parameter = names(scn), value = unname(scn))
  )
  rows <- rows[order(match(rows$subject, c(tkeys, "scalar")),
                     match(rows$parameter, c("conc", "auc", "aumc", names(scn)))), ]
  base <- paste0("model_pk2_", name)
  write_lines_lf(c("subject,parameter,value",
                   sprintf("%s,%s,%s", rows$subject, rows$parameter, num17(rows$value))),
                 file.path(exp_dir, paste0(base, ".csv")))
  if (is.null(textbook_ok)) textbook_ok <- textbook_flag(q, extra)
  meta <- list(
    schema = 1L, kind = "model", case = base,
    generated_by = "oracle/scripts/models_2c_closed_form.R",
    model = model,
    parameterisation = set,
    dose = dose,
    parameters = c(p, extra[setdiff(names(extra), "")]),
    times = arr(times),
    units = list(dose = "mg", time = "h", concentration = "mg/L"),
    note = note,
    textbook_double_ok = textbook_ok,
    quantities = list(
      conc = "concentration in the central compartment (per volume of the central compartment, apparent for extravascular input), 0 before the dose and before the lag; the bolus at t = 0 is D / vc",
      auc = "area under the concentration curve from the dose time (not from the lag) to the time",
      aumc = "area under the first moment curve, the integral of s * C(s) from the dose time to the time",
      scalar = "alpha > beta; k10, k12, k21; cl, vc, q, vp; a, b (the INTRAVENOUS coefficients D*w/vc for every route, MOD-2C-02); w_alpha, w_beta; half_life = ln2/beta, half_life_alpha = ln2/alpha; vss = vc + vp; vz = cl/beta; v_extrap = D/b; auc_inf = D/cl; aumc_inf = D*vss/cl^2 + (D/cl)*m; mrt_system = vss/cl; mrt = mrt_system + m (m = 0 bolus, dur/2 (+ tlag) zero-order, 1/ka + tlag first-order); c0 (bolus); tmax_pred, cmax_pred (not the bolus; absent for a zero dose with first-order input); a_oral, b_oral (MOD-2C-09 coefficients of exp(-alpha t) and exp(-beta t) of the oral profile, only when ka differs from alpha and from beta by at least 1e-2 relative to ka)"
    ),
    method = list(
      closed_form = "independent implementation of the explicit sums of exponentials (MOD-2C-07, 08, 09, 10), 256-bit arithmetic (Rmpfr), integrals of the terms for AUC and AUMC, rounded once to double; the exponents are the roots of the textbook quadratic, the weights (alpha - k21)/(alpha - beta); the confluent limit is used when |ka - alpha| or |ka - beta| is below 2^-150",
      cross_checks = list("matrix exponential (expm) of the 6-state system on intervals of constant input", "adaptive ODE solver (deSolve lsoda, rtol 1e-12) with AUC and first-moment states", "eigenvalues of the 2 x 2 system (exponents, when not nearly equal)"),
      cross_check_tolerance = 1e-8
    ),
    versions = versions,
    n_values = nrow(rows),
    quantities_per_time = 3L,
    n_times = length(times)
  )
  write_lines_lf(jenc(meta), file.path(exp_dir, paste0(base, ".options.json")))
  cat(sprintf("%-46s %3d times %4d values\n", base, length(times), nrow(rows)))
  nrow(rows)
}

`%||%` <- function(a, b) if (is.null(a)) b else a

versions <- list(
  R = R.version.string,
  Rmpfr = as.character(packageVersion("Rmpfr")),
  expm = as.character(packageVersion("expm")),
  deSolve = as.character(packageVersion("deSolve")),
  gmp = as.character(packageVersion("gmp"))
)

# ---------------------------------------------------------------- parameter points
# Each point is given in one set (its native one); the others are derived in 256 bits and rounded
# to double. Doses in mg, volumes in L, clearances in L/h, rates in 1/h.
ab_ratio <- {
  # alpha = 1, beta = 1e-3, weight 0.3, Vc = 20 -> clearance set rounded to 10 significant digits
  qq <- derive_q("psi", list(V = 20, alpha = 1, beta = 1e-3, w = 0.3), 400)
  list(cl = signif(num(qq$k10 * 20), 10), vc = 20, q = signif(num(qq$k12 * 20), 10),
       vp = signif(num(qq$k12 * 20 / qq$k21), 10))
}
points <- list(
  base = list(set = "clearance", p = list(cl = 2, vc = 10, q = 4, vp = 8), dose = 100),
  distribution = list(set = "clearance", p = list(cl = 1.2, vc = 6, q = 30, vp = 45), dose = 250),
  small_exchange = list(set = "clearance", p = list(cl = 3, vc = 15, q = 0.3, vp = 40), dose = 50),
  ab_ratio_1e3 = list(set = "clearance", p = ab_ratio, dose = 400),
  near_degenerate_1e3 = list(set = "clearance", p = list(cl = 2, vc = 10, q = 1e-2, vp = 5e-2), dose = 100),
  near_degenerate_1e6 = list(set = "clearance", p = list(cl = 2, vc = 10, q = 1e-5, vp = 5e-5), dose = 100),
  near_degenerate_1e9 = list(set = "micro", p = list(k10 = 0.2, k12 = 1e-9, k21 = 0.2, vc = 10), dose = 100),
  near_degenerate_1e12 = list(set = "micro", p = list(k10 = 0.2, k12 = 1e-12, k21 = 0.2, vc = 10), dose = 100),
  k10_ne_k21_1e9 = list(set = "micro", p = list(k10 = 0.2, k12 = 1e-9, k21 = 0.5, vc = 10), dose = 100),
  k10_ne_k21_1e12 = list(set = "micro", p = list(k10 = 0.2, k12 = 1e-12, k21 = 0.5, vc = 10), dose = 100),
  extreme_slow = list(set = "clearance", p = list(cl = 1e-3, vc = 1e3, q = 5e-4, vp = 2e3), dose = 1e4),
  extreme_fast = list(set = "clearance", p = list(cl = 500, vc = 0.5, q = 300, vp = 0.3), dose = 0.01),
  zero_dose = list(set = "clearance", p = list(cl = 2, vc = 10, q = 4, vp = 8), dose = 0)
)
# Parameters of a point in a requested set.
point_in <- function(name, set) {
  pt <- points[[name]]
  if (pt$set == set) return(pt$p)
  q <- derive_q(pt$set, pt$p, pt$dose)
  switch(set,
    clearance = list(cl = num(q$cl), vc = num(q$vc), q = num(q$qq), vp = num(q$vp)),
    micro = list(k10 = num(q$k10), k12 = num(q$k12), k21 = num(q$k21), vc = num(q$vc)),
    macro = list(a = num(q$A), b = num(q$B), alpha = num(q$alpha), beta = num(q$beta)))
}
alpha_of <- function(name) num(derive_q(points[[name]]$set, points[[name]]$p, points[[name]]$dose)$alpha)
beta_of <- function(name) num(derive_q(points[[name]]$set, points[[name]]$p, points[[name]]$dose)$beta)

# ---------------------------------------------------------------- the value cases
value_cases <- list()
add_case <- function(id, name, point, set, extra = list(), note = "") {
  pt <- points[[point]]
  value_cases[[length(value_cases) + 1L]] <<- list(
    name = paste0(id, "_", name), model = paste0("pk2.", id), set = set, p = point_in(point, set),
    dose = pt$dose, extra = extra, note = note, point = point)
}

# IV bolus
for (pt in c("base", "distribution", "small_exchange", "ab_ratio_1e3", "near_degenerate_1e6")) {
  for (st in c("clearance", "micro", "macro")) {
    add_case("iv_bolus", if (st == "clearance") pt else paste0(pt, "_", st), pt, st,
             note = paste0("IV bolus, point ", pt, " given in the ", st, " set"))
  }
}
add_case("iv_bolus", "near_degenerate_1e3", "near_degenerate_1e3", "clearance", note = "k12 = 1e-3, k10 = k21 = 0.2")
add_case("iv_bolus", "near_degenerate_1e9_micro", "near_degenerate_1e9", "micro", note = "worked example N6, k12 = 1e-9, k10 = k21")
add_case("iv_bolus", "near_degenerate_1e12_micro", "near_degenerate_1e12", "micro", note = "k12 = 1e-12, k10 = k21")
add_case("iv_bolus", "k10_ne_k21_1e9_micro", "k10_ne_k21_1e9", "micro", note = "k12 = 1e-9, k10 = 0.2 and k21 = 0.5 (a very small positive weight)")
add_case("iv_bolus", "k10_ne_k21_1e12_micro", "k10_ne_k21_1e12", "micro", note = "k12 = 1e-12, k10 != k21")
add_case("iv_bolus", "extreme_slow", "extreme_slow", "clearance", note = "rates of order 1e-6 per hour")
add_case("iv_bolus", "extreme_fast", "extreme_fast", "clearance", note = "rates of order 1e3 per hour")
add_case("iv_bolus", "zero_dose", "zero_dose", "clearance", note = "dose 0 gives 0 everywhere")
add_case("iv_bolus", "zero_dose_micro", "zero_dose", "micro", note = "dose 0, micro set")

# IV infusion
inf_dur <- c(base = 2, distribution = 0.5, small_exchange = 10, ab_ratio_1e3 = 50, near_degenerate_1e6 = 4)
for (pt in names(inf_dur)) {
  for (st in c("clearance", "micro", "macro")) {
    if (st != "clearance" && !(pt %in% c("base", "distribution", "near_degenerate_1e6"))) next
    add_case("iv_infusion", if (st == "clearance") pt else paste0(pt, "_", st), pt, st,
             extra = list(dur = inf_dur[[pt]]),
             note = paste0("IV infusion over ", inf_dur[[pt]], " h, point ", pt, " in the ", st, " set"))
  }
}
add_case("iv_infusion", "near_degenerate_1e9_micro", "near_degenerate_1e9", "micro", list(dur = 4), "k12 = 1e-9, T = 4")
add_case("iv_infusion", "extreme_slow", "extreme_slow", "clearance", list(dur = 1e5), "rates of order 1e-6, T = 1e5 h")
add_case("iv_infusion", "extreme_fast", "extreme_fast", "clearance", list(dur = 1e-3), "rates of order 1e3, T = 1e-3 h")
add_case("iv_infusion", "short_duration", "base", "clearance", list(dur = 1e-3), "T = 1e-3 h: close to the bolus (MOD-2C-08)")
add_case("iv_infusion", "long_duration", "base", "clearance", list(dur = 300), "T = 300 h: the plateau R0 / CL is reached")
add_case("iv_infusion", "zero_dose", "zero_dose", "clearance", list(dur = 2), "dose 0")

# First-order absorption, no lag
a1 <- alpha_of("base"); b1 <- beta_of("base")
for (st in c("clearance", "micro", "macro")) {
  add_case("oral_1", if (st == "clearance") "base" else paste0("base_", st), "base", st, list(ka = 2),
           paste0("worked example N3, ka = 2, ", st, " set"))
}
add_case("oral_1", "ka_eq_alpha_macro", "base", "macro", list(ka = point_in("base", "macro")$alpha), "ka = alpha exactly (the confluent limit of MOD-2C-10)")
add_case("oral_1", "ka_eq_beta_macro", "base", "macro", list(ka = point_in("base", "macro")$beta), "ka = beta exactly")
for (i in 1:3) {
  d <- c(1e-3, 1e-6, 1e-9)[i]
  tag <- c("1e3", "1e6", "1e9")[i]
  add_case("oral_1", paste0("ka_alpha_plus_", tag), "base", "clearance", list(ka = 1 + d), paste0("ka = alpha + ", d))
  add_case("oral_1", paste0("ka_alpha_minus_", tag), "base", "clearance", list(ka = 1 - d), paste0("ka = alpha - ", d))
  add_case("oral_1", paste0("ka_beta_plus_", tag), "base", "clearance", list(ka = 0.1 + d), paste0("ka = beta + ", d))
  add_case("oral_1", paste0("ka_beta_minus_", tag), "base", "clearance", list(ka = 0.1 - d), paste0("ka = beta - ", d))
}
add_case("oral_1", "ka_between", "base", "clearance", list(ka = 0.4), "beta < ka < alpha: the oral coefficient of exp(-alpha t) is negative")
add_case("oral_1", "ka_below_beta", "base", "clearance", list(ka = 0.05), "ka < beta: flip-flop shape")
add_case("oral_1", "ka_500_alpha", "base", "clearance", list(ka = 500), "ka / alpha = 500")
add_case("oral_1", "ka_eq_k10", "base", "clearance", list(ka = 0.2), "ka = k10")
add_case("oral_1", "ka_eq_k12", "base", "clearance", list(ka = 0.4), "ka = k12")
add_case("oral_1", "ka_eq_k21", "base", "clearance", list(ka = 0.5), "ka = k21")
add_case("oral_1", "distribution", "distribution", "clearance", list(ka = 3), "distribution phase, ka = 3")
add_case("oral_1", "small_exchange", "small_exchange", "clearance", list(ka = 0.5), "small exchange, ka = 0.5")
add_case("oral_1", "ab_ratio_1e3", "ab_ratio_1e3", "clearance", list(ka = 5), "alpha / beta = 1e3, ka = 5")
add_case("oral_1", "near_degenerate_1e6", "near_degenerate_1e6", "clearance", list(ka = 0.2), "k12 = 1e-6, ka = 0.2 (close to both exponents)")
add_case("oral_1", "near_degenerate_1e9_micro", "near_degenerate_1e9", "micro", list(ka = 0.2), "k12 = 1e-9, ka = 0.2")
add_case("oral_1", "near_degenerate_1e12_micro", "near_degenerate_1e12", "micro", list(ka = 0.2), "k12 = 1e-12, ka = 0.2")
add_case("oral_1", "near_degenerate_1e9_fast_ka", "near_degenerate_1e9", "micro", list(ka = 5), "k12 = 1e-9, ka = 5")
add_case("oral_1", "extreme_slow", "extreme_slow", "clearance", list(ka = 1e-5), "rates of order 1e-6, ka = 1e-5")
add_case("oral_1", "extreme_fast", "extreme_fast", "clearance", list(ka = 5e3), "rates of order 1e3, ka = 5e3")
add_case("oral_1", "zero_dose", "zero_dose", "clearance", list(ka = 2), "dose 0")

# First-order absorption with lag
for (st in c("clearance", "micro", "macro")) {
  add_case("oral_1_lag", if (st == "clearance") "base" else paste0("base_", st), "base", st, list(ka = 2, tlag = 0.5),
           paste0("worked example N5, ka = 2, tlag = 0.5, ", st, " set"))
}
add_case("oral_1_lag", "ka_eq_alpha_macro", "base", "macro", list(ka = point_in("base", "macro")$alpha, tlag = 0.5), "ka = alpha exactly, tlag = 0.5")
add_case("oral_1_lag", "distribution", "distribution", "clearance", list(ka = 3, tlag = 0.25), "distribution phase")
add_case("oral_1_lag", "small_exchange", "small_exchange", "clearance", list(ka = 0.5, tlag = 2), "small exchange")
add_case("oral_1_lag", "ab_ratio_1e3", "ab_ratio_1e3", "clearance", list(ka = 5, tlag = 3), "alpha / beta = 1e3")
add_case("oral_1_lag", "near_degenerate_1e6", "near_degenerate_1e6", "clearance", list(ka = 0.2, tlag = 0.5), "k12 = 1e-6, ka = 0.2")
add_case("oral_1_lag", "long_lag", "base", "clearance", list(ka = 2, tlag = 24), "tlag = 24 h")
add_case("oral_1_lag", "zero_dose", "zero_dose", "clearance", list(ka = 2, tlag = 0.5), "dose 0")

# Zero-order absorption, no lag
for (pt in names(inf_dur)) {
  for (st in c("clearance", "micro", "macro")) {
    if (st != "clearance" && !(pt %in% c("base", "distribution", "near_degenerate_1e6"))) next
    add_case("oral_0", if (st == "clearance") pt else paste0(pt, "_", st), pt, st, list(dur = inf_dur[[pt]]),
             paste0("zero-order absorption over ", inf_dur[[pt]], " h, point ", pt, " in the ", st, " set"))
  }
}
add_case("oral_0", "near_degenerate_1e9_micro", "near_degenerate_1e9", "micro", list(dur = 4), "k12 = 1e-9, T = 4")
add_case("oral_0", "extreme_slow", "extreme_slow", "clearance", list(dur = 1e5), "rates of order 1e-6, T = 1e5 h")
add_case("oral_0", "extreme_fast", "extreme_fast", "clearance", list(dur = 1e-3), "rates of order 1e3, T = 1e-3 h")
add_case("oral_0", "zero_dose", "zero_dose", "clearance", list(dur = 2), "dose 0")

# Zero-order absorption with lag
add_case("oral_0_lag", "base", "base", "clearance", list(dur = 2, tlag = 0.5), "worked example N5, T = 2, tlag = 0.5")
add_case("oral_0_lag", "base_micro", "base", "micro", list(dur = 2, tlag = 0.5), "micro set")
add_case("oral_0_lag", "base_macro", "base", "macro", list(dur = 2, tlag = 0.5), "macro set")
add_case("oral_0_lag", "distribution", "distribution", "clearance", list(dur = 0.5, tlag = 0.25), "distribution phase")
add_case("oral_0_lag", "ab_ratio_1e3", "ab_ratio_1e3", "clearance", list(dur = 50, tlag = 3), "alpha / beta = 1e3")
add_case("oral_0_lag", "near_degenerate_1e6", "near_degenerate_1e6", "clearance", list(dur = 4, tlag = 0.5), "k12 = 1e-6")
add_case("oral_0_lag", "extreme_fast", "extreme_fast", "clearance", list(dur = 1e-3, tlag = 5e-4), "rates of order 1e3")
add_case("oral_0_lag", "zero_dose", "zero_dose", "clearance", list(dur = 2, tlag = 0.5), "dose 0")

total_values <- 0L
for (cs in value_cases) {
  total_values <- total_values + write_case(cs$name, cs$model, cs$set, cs$p, cs$dose, cs$extra, cs$note)
}
cat(sprintf("value cases: %d, %d values\n", length(value_cases), total_values))

source(file.path(root, "oracle", "scripts", "models_2c_derivatives.R"))
